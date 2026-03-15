use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use herald_core::config::HeraldConfig;
use herald_core::ipc::protocol::{IpcRequest, IpcResponse};
use herald_core::ipc::server::{ConnectionInfo, IpcServer};
use herald_core::logging::ConversationLogger;
use herald_core::security::content_filter::filter_content;
use herald_core::session::registry::SessionRegistry;
use herald_core::session::token::generate_token;
use herald_core::telegram::bot::{
    create_bot, drain_queue, enqueue_message, enqueue_message_with_keyboard, run_bot, BotState,
    OutboundMessage, PendingPermission, PendingPermissions, PendingQuestion, PendingQuestions,
    QuestionOption,
};
use herald_core::telegram::callbacks::{build_permission_keyboard, build_question_keyboard};
use herald_core::telegram::formatting::{
    escape_markdown_v2, format_ask_user_question, format_ask_user_question_with_options,
    format_completion, format_permission_request, format_session_end, format_session_start,
    format_working, split_message,
};
use herald_core::types::{ConversationEntry, SessionId, SessionInfo, SessionModes, SessionState, TokenUsage};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Per-session activity state for debounce-based completion detection
struct ActivityState {
    working: bool,
    tool_count: u32,
    prompt_preview: String,
    completion_handle: Option<JoinHandle<()>>,
    last_assistant_response: Option<String>,
}

type ActivityTracker = Arc<Mutex<HashMap<String, ActivityState>>>;

pub async fn run(config: HeraldConfig) -> Result<()> {
    let start_time = chrono::Utc::now();
    let config_path = HeraldConfig::default_path();
    let registry = SessionRegistry::new();
    let telegram_connected = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // Clean up stale token files on startup
    cleanup_token_files();

    // Create conversation logger with configured output mode
    let log_output = herald_core::logging::LogOutput::from_str(&config.daemon.log_output);
    let logger = Arc::new(ConversationLogger::new(None, log_output));

    // Read debounce from config
    let debounce_secs = config.sessions.debounce_seconds;

    // Create message queue
    let (queue_tx, queue_rx) = mpsc::channel::<OutboundMessage>(256);

    // Create bot and shared state
    let bot_token = config.get_bot_token()?;
    let bot = create_bot(&bot_token);

    let config_arc = Arc::new(RwLock::new(config));

    let pending_permissions: PendingPermissions = Arc::new(Mutex::new(HashMap::new()));
    let pending_questions: PendingQuestions = Arc::new(Mutex::new(HashMap::new()));

    let bot_state = BotState {
        config: config_arc.clone(),
        config_path: config_path.clone(),
        registry: registry.clone(),
        queue_tx: queue_tx.clone(),
        telegram_connected: telegram_connected.clone(),
        start_time,
        pending_otp: Arc::new(Mutex::new(None)),
        active_session: Arc::new(Mutex::new(None)),
        pending_permissions: pending_permissions.clone(),
        pending_questions: pending_questions.clone(),
    };

    // Task 1: IPC server
    let config_for_ipc = config_arc.clone();
    let registry_for_ipc = registry.clone();
    let queue_tx_for_ipc = queue_tx.clone();
    let telegram_connected_for_ipc = telegram_connected.clone();
    let shutdown_tx_clone = shutdown_tx.clone();

    let mut ipc_server = {
        let config = config_for_ipc.read().await;
        IpcServer::bind_from_config(&config.daemon).await?
    };

    let activity: ActivityTracker = Arc::new(Mutex::new(HashMap::new()));

    let logger_for_ipc = logger.clone();
    let activity_for_ipc = activity.clone();
    let permissions_for_ipc = pending_permissions.clone();
    let questions_for_ipc = pending_questions.clone();
    let ipc_handle = tokio::spawn(async move {
        let registry = registry_for_ipc;
        let config = config_for_ipc;
        let queue_tx = queue_tx_for_ipc;
        let tc = telegram_connected_for_ipc;
        let start = start_time;
        let shutdown = shutdown_tx_clone;
        let logger = logger_for_ipc;
        let activity = activity_for_ipc;
        let permissions = permissions_for_ipc;
        let questions = questions_for_ipc;

        if let Err(e) = ipc_server
            .run(move |request, conn_info| {
                let registry = registry.clone();
                let config = config.clone();
                let queue_tx = queue_tx.clone();
                let tc = tc.clone();
                let shutdown = shutdown.clone();
                let logger = logger.clone();
                let activity = activity.clone();
                let permissions = permissions.clone();
                let questions = questions.clone();
                async move {
                    handle_request(request, conn_info, &registry, &config, &queue_tx, &tc, start, &shutdown, &logger, &activity, debounce_secs, &permissions, &questions)
                        .await
                }
            })
            .await
        {
            error!("IPC server error: {}", e);
        }
    });

    // Task 2: Telegram bot polling
    let bot_clone = bot.clone();
    let bot_state_clone = bot_state.clone();
    let telegram_handle = tokio::spawn(async move {
        run_bot(bot_clone, bot_state_clone).await;
    });

    // Task 3: Message queue drain
    let bot_for_queue = bot.clone();
    let queue_handle = tokio::spawn(async move {
        drain_queue(queue_rx, bot_for_queue).await;
    });

    // Task 4: Signal handler
    let signal_handle = tokio::spawn(async move {
        crate::signal::wait_for_shutdown().await;
        let _ = shutdown_tx.send(true);
    });

    // Task 5: Garbage session reaper (check every 60s for dead processes)
    let registry_for_reaper = registry.clone();
    let config_for_reaper = config_arc.clone();
    let queue_tx_for_reaper = queue_tx.clone();
    let activity_for_reaper = activity.clone();
    let permissions_for_reaper = pending_permissions.clone();
    let reaper_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            reap_dead_sessions(
                &registry_for_reaper,
                &config_for_reaper,
                &queue_tx_for_reaper,
                &activity_for_reaper,
                &permissions_for_reaper,
            )
            .await;
            // Reap stale permission requests (>60s old)
            reap_stale_permissions(&permissions_for_reaper).await;
        }
    });

    // Wait for shutdown signal
    shutdown_rx.changed().await?;
    info!("Shutting down Herald daemon...");

    // Notify Telegram users
    {
        let config = config_arc.read().await;
        for &chat_id in &config.auth.allowed_chat_ids {
            enqueue_message(&queue_tx, chat_id, "Herald daemon shutting down.".to_string(), None)
                .await;
        }
    }

    // Brief delay to flush queue
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    ipc_handle.abort();
    telegram_handle.abort();
    queue_handle.abort();
    signal_handle.abort();
    reaper_handle.abort();

    Ok(())
}

/// Clean up stale token files on daemon startup
fn cleanup_token_files() {
    let token_dir = std::path::Path::new("/tmp/herald/tokens");
    if token_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(token_dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        info!("Cleaned up stale token files");
    }
}

/// Reap sessions whose processes have died (garbage collection)
async fn reap_dead_sessions(
    registry: &SessionRegistry,
    config: &Arc<RwLock<HeraldConfig>>,
    queue_tx: &mpsc::Sender<OutboundMessage>,
    activity: &ActivityTracker,
    pending_permissions: &PendingPermissions,
) {
    let sessions = registry.list().await;
    for session in &sessions {
        let should_reap = !crate::pty::is_process_alive(session.pid);
        if should_reap {
            let tag = registry.get_tag(&session.id).await;
            info!("Reaping session {} (process exited)", session.id);

            // Clean up activity state
            {
                let mut act = activity.lock().await;
                if let Some(state) = act.remove(&session.id) {
                    if let Some(handle) = state.completion_handle {
                        handle.abort();
                    }
                }
            }

            // Unregister
            let _ = registry.unregister(&session.id).await;

            // Clean up pending permissions for this session
            {
                let mut perms = pending_permissions.lock().await;
                perms.retain(|_id, perm| perm.session_id != *session.id);
            }

            // Clean up token file
            let token_file = format!("/tmp/herald/tokens/{}", session.id);
            let _ = std::fs::remove_file(&token_file);

            // Notify Telegram about unexpected process death
            {
                let config = config.read().await;
                let text = format!("{} Session lost \\(process exited\\)", escape_markdown_v2(&tag));
                for &chat_id in &config.auth.allowed_chat_ids {
                    enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                }
            }
        }
    }
}

/// Check if token auth should be skipped (Unix socket with peercred verification)
fn skip_token_auth(conn_info: &ConnectionInfo) -> bool {
    conn_info.is_unix && conn_info.peercred_verified
}

/// Validate token for a request, skipping validation for peercred-verified Unix connections
async fn validate_token(
    registry: &SessionRegistry,
    session_id: &str,
    token: &Option<String>,
    conn_info: &ConnectionInfo,
    context: &str,
) -> std::result::Result<(), IpcResponse> {
    if skip_token_auth(conn_info) {
        // Unix: skip token check but verify session exists in registry
        if registry.get(session_id).await.is_none() {
            return Err(IpcResponse::Error {
                code: 410,
                message: "Session not registered".to_string(),
            });
        }
        return Ok(());
    }
    match token {
        Some(t) if registry.validate_token(session_id, t).await => Ok(()),
        _ => {
            warn!("{}: invalid token for session {}", context, session_id);
            Err(IpcResponse::Error {
                code: 401,
                message: "Invalid session token".to_string(),
            })
        }
    }
}

async fn handle_request(
    request: IpcRequest,
    conn_info: ConnectionInfo,
    registry: &SessionRegistry,
    config: &Arc<RwLock<HeraldConfig>>,
    queue_tx: &mpsc::Sender<OutboundMessage>,
    telegram_connected: &Arc<AtomicBool>,
    start_time: chrono::DateTime<chrono::Utc>,
    shutdown_tx: &watch::Sender<bool>,
    logger: &ConversationLogger,
    activity: &ActivityTracker,
    debounce_secs: u64,
    pending_permissions: &PendingPermissions,
    pending_questions: &PendingQuestions,
) -> IpcResponse {
    match request {
        IpcRequest::Register {
            session_id,
            pid,
            cwd,
            tmux_pane,
        } => {
            let token = generate_token();
            let display_name = SessionInfo::name_from_cwd(&cwd);
            let color_index = registry.next_color();
            let has_tmux = tmux_pane.is_some();
            let info = SessionInfo {
                id: SessionId(session_id.clone()),
                token: token.clone(),
                pid,
                cwd: cwd.clone().into(),
                display_name: display_name.clone(),
                color_index,
                state: SessionState::Active,
                started_at: chrono::Utc::now(),
                last_activity: chrono::Utc::now(),
                token_usage: TokenUsage::default(),
                conversation_log: Vec::new(),
                tmux_pane,
                modes: SessionModes {
                    bypass_permissions: config.read().await.sessions.default_bypass_permissions,
                    ..Default::default()
                },
            };
            let tag = info.tag();
            match registry.register(info).await {
                Ok(()) => {
                    info!("Session registered: {} (PID {}, tmux={})", session_id, pid, has_tmux);
                    logger.log_session_event(&session_id, &format!("started ({})", cwd));
                    let config = config.read().await;
                    let text = format_session_start(&tag, &cwd);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                    }
                    IpcResponse::Registered { token: token.0 }
                }
                Err(e) => IpcResponse::Error {
                    code: 500,
                    message: e.to_string(),
                },
            }
        }
        IpcRequest::Unregister { session_id, token } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, "Unregister").await {
                return resp;
            }
            // Clean up activity state (debounce timer, working state)
            {
                let mut act = activity.lock().await;
                if let Some(state) = act.remove(&session_id) {
                    if let Some(handle) = state.completion_handle {
                        handle.abort();
                    }
                }
            }
            // Clean up pending permissions for this session
            {
                let mut perms = pending_permissions.lock().await;
                perms.retain(|_id, perm| perm.session_id != session_id);
            }
            let tag = registry.get_tag(&session_id).await;
            match registry.unregister(&session_id).await {
                Ok(()) => {
                    info!("Session unregistered: {}", session_id);
                    logger.log_session_event(&session_id, "ended");
                    let config = config.read().await;
                    let text = format_session_end(&tag);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                    }
                    IpcResponse::Ok {
                        message: Some("Session unregistered".to_string()),
                    }
                }
                Err(e) => IpcResponse::Error {
                    code: 404,
                    message: e.to_string(),
                },
            }
        }
        IpcRequest::Output {
            session_id,
            token,
            tool_name,
            tool_input_summary,
            tool_response_summary: _,
        } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, "Output").await {
                return resp;
            }
            registry.update_activity(&session_id).await;

            // Send compact tool activity to Telegram
            {
                let tag = registry.get_tag(&session_id).await;
                let input_preview: String = tool_input_summary
                    .lines().next().unwrap_or("")
                    .chars().take(100).collect();
                let text = if input_preview.is_empty() {
                    format!("{} \u{2699}\u{fe0f} `{}`",
                        escape_markdown_v2(&tag),
                        escape_markdown_v2(&tool_name),
                    )
                } else {
                    format!("{} \u{2699}\u{fe0f} `{}`\n> {}",
                        escape_markdown_v2(&tag),
                        escape_markdown_v2(&tool_name),
                        escape_markdown_v2(&input_preview),
                    )
                };
                let config = config.read().await;
                for &chat_id in &config.auth.allowed_chat_ids {
                    enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                }
            }

            // Increment tool count and reset debounce timer
            let mut act = activity.lock().await;
            let state = act.entry(session_id.clone()).or_insert_with(|| {
                info!("Output before user_prompt for session {} (tool: {}), creating activity state", session_id, tool_name);
                ActivityState {
                    working: true,
                    tool_count: 0,
                    prompt_preview: String::new(),
                    completion_handle: None,
                    last_assistant_response: None,
                }
            });
            state.tool_count += 1;
            // Cancel existing timer
            if let Some(handle) = state.completion_handle.take() {
                handle.abort();
            }
            // Start new debounce timer
            let session_id_clone = session_id.clone();
            let registry_clone = registry.clone();
            let config_clone = config.clone();
            let queue_tx_clone = queue_tx.clone();
            let activity_clone = activity.clone();
            state.completion_handle = Some(tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(debounce_secs)).await;
                fire_completion(
                    &session_id_clone,
                    &registry_clone,
                    &config_clone,
                    &queue_tx_clone,
                    &activity_clone,
                )
                .await;
            }));

            IpcResponse::Ok { message: None }
        }
        IpcRequest::Notification {
            session_id,
            token,
            notification_type,
            message,
            extras,
        } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, "Notification").await {
                return resp;
            }

            let tag = registry.get_tag(&session_id).await;
            let config = config.read().await;

            if notification_type == "ask_user_question" {
                // Try to parse structured question options from extras
                let mut sent_with_keyboard = false;
                if let Some(ref extras_json) = extras {
                    if let Ok(questions) = serde_json::from_str::<Vec<serde_json::Value>>(extras_json) {
                        if let Some(first_q) = questions.first() {
                            let options: Vec<(String, String)> = first_q
                                .get("options")
                                .and_then(|o| o.as_array())
                                .map(|arr| {
                                    arr.iter().map(|opt| {
                                        let label = opt.get("label").and_then(|l| l.as_str()).unwrap_or("").to_string();
                                        let desc = opt.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
                                        (label, desc)
                                    }).collect()
                                })
                                .unwrap_or_default();

                            if !options.is_empty() {
                                let request_id = format!("q{}",
                                    chrono::Utc::now().timestamp_millis() % 1_000_000);

                                // Store pending question
                                {
                                    let mut pqs = pending_questions.lock().await;
                                    pqs.insert(request_id.clone(), PendingQuestion {
                                        session_id: session_id.clone(),
                                        question_text: message.clone(),
                                        options: options.iter().map(|(l, d)| QuestionOption {
                                            label: l.clone(),
                                            description: d.clone(),
                                        }).collect(),
                                        created_at: chrono::Utc::now(),
                                    });
                                }

                                let text = format_ask_user_question_with_options(&tag, &message, &options);
                                let keyboard = build_question_keyboard(&request_id, &options);

                                for &chat_id in &config.auth.allowed_chat_ids {
                                    enqueue_message_with_keyboard(
                                        queue_tx, chat_id, text.clone(),
                                        Some("MarkdownV2".to_string()), keyboard.clone(),
                                    ).await;
                                }
                                sent_with_keyboard = true;
                            }
                        }
                    }
                }

                // Fallback: no structured options, show plain question
                if !sent_with_keyboard {
                    let text = format_ask_user_question(&tag, &message);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                    }
                }
            } else {
                let text = format!(
                    "{} Notification \\[{}\\]:\n{}",
                    escape_markdown_v2(&tag),
                    escape_markdown_v2(&notification_type),
                    escape_markdown_v2(&message)
                );
                for &chat_id in &config.auth.allowed_chat_ids {
                    enqueue_message(
                        queue_tx,
                        chat_id,
                        text.clone(),
                        Some("MarkdownV2".to_string()),
                    )
                    .await;
                }
            }
            IpcResponse::Ok { message: None }
        }
        IpcRequest::SessionStopped {
            session_id,
            token,
            last_message,
        } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, "SessionStopped").await {
                return resp;
            }

            // Extract activity state and cancel debounce timer
            let activity_info = {
                let mut act = activity.lock().await;
                act.remove(&session_id).map(|state| {
                    if let Some(handle) = state.completion_handle {
                        handle.abort();
                    }
                    (state.tool_count, state.last_assistant_response)
                })
            };

            let tag = registry.get_tag(&session_id).await;

            // Get token usage (session stays Active — only Unregister marks true end)
            let usage = registry.get_token_usage(&session_id).await;

            // Build rich completion message
            let (tool_count, stored_response) = activity_info.unwrap_or((0, None));

            // Use stored assistant_response, fall back to last_message from hook
            let response_content = stored_response
                .filter(|s| !s.is_empty())
                .unwrap_or(last_message);

            let text = format_completion(&tag, tool_count, usage.as_ref(), &response_content);

            let config = config.read().await;
            for &chat_id in &config.auth.allowed_chat_ids {
                let parts = split_message(&text, 4096);
                for part in parts {
                    enqueue_message(queue_tx, chat_id, part, Some("MarkdownV2".to_string())).await;
                }
            }
            IpcResponse::Ok { message: None }
        }
        IpcRequest::Input {
            session_id,
            prompt,
        } => {
            // Headless mode: use `claude --continue <session_id> -p <prompt>`
            let config = config.read().await;
            match crate::headless::execute_prompt(&prompt, Some(&session_id)).await {
                Ok(output) => {
                    info!("Headless output ({} chars): {}", output.len(), &output[..output.len().min(200)]);
                    let filtered = filter_content(&output, &config.output_filter);
                    info!("Filtered output ({} chars): {}", filtered.len(), &filtered[..filtered.len().min(200)]);
                    let text = if filtered.trim().is_empty() {
                        warn!("Headless response is empty after filtering");
                        "(No response from Claude)".to_string()
                    } else {
                        filtered
                    };
                    for &chat_id in &config.auth.allowed_chat_ids {
                        let parts = split_message(&text, 4096);
                        for part in parts {
                            enqueue_message(queue_tx, chat_id, part, None).await;
                        }
                    }
                    IpcResponse::Ok {
                        message: Some("Prompt executed".to_string()),
                    }
                }
                Err(e) => IpcResponse::Error {
                    code: 500,
                    message: format!("Headless execution failed: {}", e),
                },
            }
        }
        IpcRequest::TokenUpdate {
            session_id,
            token,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            total_cost_usd,
        } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, "TokenUpdate").await {
                return resp;
            }

            let usage = TokenUsage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                total_cost_usd,
            };

            registry.update_token_usage(&session_id, usage.clone()).await;
            logger.log_token_usage(&session_id, &usage);

            // Suppress Telegram message while session is working (included in completion summary)
            let is_working = {
                let act = activity.lock().await;
                act.get(&session_id).map(|s| s.working).unwrap_or(false)
            };
            if !is_working {
                let tag = registry.get_tag(&session_id).await;
                let config = config.read().await;
                let text = format!(
                    "{} \u{1f4ca} Tokens\n\
                     \u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\n\
                     `{} in / {} out`\n\
                     `Cache: {} read / {} created`\n\
                     `Cost: ${:.4}`",
                    escape_markdown_v2(&tag),
                    format_num(input_tokens),
                    format_num(output_tokens),
                    format_num(cache_read_tokens),
                    format_num(cache_creation_tokens),
                    total_cost_usd,
                );
                for &chat_id in &config.auth.allowed_chat_ids {
                    enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                }
            }

            IpcResponse::Ok { message: None }
        }
        IpcRequest::ConversationEntry {
            session_id,
            token,
            entry_type,
            content,
            timestamp,
        } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, &format!("ConversationEntry({})", entry_type)).await {
                return resp;
            }

            let ts = chrono::DateTime::parse_from_rfc3339(&timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let entry = ConversationEntry {
                timestamp: ts,
                entry_type: entry_type.clone(),
                content: content.clone(),
            };

            registry.add_conversation_entry(&session_id, entry).await;

            // Log to file and send to Telegram with session tag
            let tag = registry.get_tag(&session_id).await;
            match entry_type.as_str() {
                "user_prompt" => {
                    logger.log_user_prompt(&session_id, &content);

                    // Set working state, cancel any existing timer
                    {
                        let mut act = activity.lock().await;
                        let state = act.entry(session_id.clone()).or_insert_with(|| ActivityState {
                            working: false,
                            tool_count: 0,
                            prompt_preview: String::new(),
                            completion_handle: None,
                            last_assistant_response: None,
                        });
                        if let Some(handle) = state.completion_handle.take() {
                            handle.abort();
                        }
                        state.working = true;
                        state.tool_count = 0;
                        state.last_assistant_response = None;
                        state.prompt_preview = safe_truncate(&content, 100);
                    }

                    let config = config.read().await;
                    let preview = {
                        let act = activity.lock().await;
                        act.get(&session_id).map(|s| s.prompt_preview.clone()).unwrap_or_default()
                    };
                    let text = format_working(&tag, &preview);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), Some("MarkdownV2".to_string())).await;
                    }
                }
                "assistant_response" => {
                    logger.log_assistant_response(&session_id, &content);
                    // Store for use by fire_completion/SessionStopped as fallback
                    // Also reset debounce timer — assistant is still active
                    let mut act = activity.lock().await;
                    if let Some(state) = act.get_mut(&session_id) {
                        state.last_assistant_response = Some(content.clone());
                        // Reset debounce: abort existing timer and start a new one
                        if let Some(handle) = state.completion_handle.take() {
                            handle.abort();
                        }
                        let session_id_clone = session_id.clone();
                        let registry_clone = registry.clone();
                        let config_clone = config.clone();
                        let queue_tx_clone = queue_tx.clone();
                        let activity_clone = activity.clone();
                        state.completion_handle = Some(tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(debounce_secs)).await;
                            fire_completion(
                                &session_id_clone,
                                &registry_clone,
                                &config_clone,
                                &queue_tx_clone,
                                &activity_clone,
                            )
                            .await;
                        }));
                    }
                }
                "tool_summary" => {
                    logger.log_tool_summary(&session_id, &content);
                    // Tool summaries suppressed during work
                }
                _ => {}
            }

            IpcResponse::Ok { message: None }
        }
        IpcRequest::PermissionRequest {
            session_id,
            token,
            request_id,
            tool_name,
            tool_input,
        } => {
            if let Err(resp) = validate_token(registry, &session_id, &token, &conn_info, "PermissionRequest").await {
                return resp;
            }

            // Bypass permissions: auto-allow without Telegram interaction
            if let Some(modes) = registry.get_modes(&session_id).await {
                if modes.bypass_permissions {
                    info!("Bypass mode: auto-allowing {} for session {}", tool_name, session_id);
                    return IpcResponse::PermissionResult {
                        decision: "allow".to_string(),
                    };
                }
            }

            let tag = registry.get_tag(&session_id).await;

            // Store pending permission
            {
                let mut perms = pending_permissions.lock().await;
                perms.insert(
                    request_id.clone(),
                    PendingPermission {
                        session_id: session_id.clone(),
                        tool_name: tool_name.clone(),
                        tool_input: tool_input.clone(),
                        decision: None,
                        created_at: chrono::Utc::now(),
                    },
                );
            }

            // Send Telegram message with approve/deny buttons
            let text = format_permission_request(&tag, &tool_name, &tool_input);
            let keyboard = build_permission_keyboard(&request_id);
            let config = config.read().await;
            for &chat_id in &config.auth.allowed_chat_ids {
                enqueue_message_with_keyboard(
                    queue_tx,
                    chat_id,
                    text.clone(),
                    Some("MarkdownV2".to_string()),
                    keyboard.clone(),
                )
                .await;
            }

            info!("Permission request {} for tool {} (session {})", request_id, tool_name, session_id);
            IpcResponse::Ok { message: None }
        }
        IpcRequest::PermissionCheck { request_id } => {
            let perms = pending_permissions.lock().await;
            if let Some(perm) = perms.get(&request_id) {
                let decision = perm.decision.clone().unwrap_or_else(|| "pending".to_string());
                IpcResponse::PermissionResult { decision }
            } else {
                // Not found — may have been reaped; auto-allow
                IpcResponse::PermissionResult {
                    decision: "allow".to_string(),
                }
            }
        }
        IpcRequest::ModeQuery { session_id } => {
            if let Some(modes) = registry.get_modes(&session_id).await {
                IpcResponse::ModeResult {
                    plan_mode: modes.plan_mode,
                    bypass_permissions: modes.bypass_permissions,
                }
            } else {
                let cfg = config.read().await;
                IpcResponse::ModeResult {
                    plan_mode: false,
                    bypass_permissions: cfg.sessions.default_bypass_permissions,
                }
            }
        }
        IpcRequest::ModeUpdate {
            session_id,
            plan_mode,
            bypass_permissions,
        } => {
            registry
                .update_modes(
                    &session_id,
                    SessionModes {
                        plan_mode,
                        bypass_permissions,
                    },
                )
                .await;
            info!(
                "Mode update for session {}: plan={}, bypass={}",
                session_id, plan_mode, bypass_permissions
            );
            IpcResponse::Ok {
                message: Some("Modes updated".to_string()),
            }
        }
        IpcRequest::Health => {
            let uptime = (chrono::Utc::now() - start_time).num_seconds() as u64;
            let session_count = registry.count().await;
            IpcResponse::HealthStatus {
                uptime_secs: uptime,
                session_count,
                telegram_connected: telegram_connected.load(Ordering::Relaxed),
            }
        }
        IpcRequest::ListSessions => {
            let sessions = registry.list().await;
            IpcResponse::SessionList { sessions }
        }
        IpcRequest::Shutdown => {
            info!("Shutdown requested via IPC");
            let _ = shutdown_tx.send(true);
            IpcResponse::Ok {
                message: Some("Shutting down".to_string()),
            }
        }
    }
}

/// Fired when the debounce timer expires — session is idle, send completion summary.
/// Removes the activity entry so SessionStopped won't duplicate the message.
async fn fire_completion(
    session_id: &str,
    registry: &SessionRegistry,
    config: &Arc<RwLock<HeraldConfig>>,
    queue_tx: &mpsc::Sender<OutboundMessage>,
    activity: &ActivityTracker,
) {
    let (tool_count, stored_response) = {
        let mut act = activity.lock().await;
        if let Some(state) = act.remove(session_id) {
            if let Some(handle) = state.completion_handle {
                // Don't abort ourselves — just drop the handle
                drop(handle);
            }
            (state.tool_count, state.last_assistant_response)
        } else {
            return;
        }
    };

    let tag = registry.get_tag(session_id).await;

    // Get token usage for summary
    let usage = registry.get_token_usage(session_id).await;

    // Use stored assistant_response from activity state
    let response_content = stored_response.unwrap_or_default();

    let text = format_completion(&tag, tool_count, usage.as_ref(), &response_content);

    let config = config.read().await;
    for &chat_id in &config.auth.allowed_chat_ids {
        let parts = split_message(&text, 4096);
        for part in parts {
            enqueue_message(queue_tx, chat_id, part, Some("MarkdownV2".to_string())).await;
        }
    }
}

/// Reap stale permission requests older than 60 seconds
async fn reap_stale_permissions(permissions: &PendingPermissions) {
    let now = chrono::Utc::now();
    let mut perms = permissions.lock().await;
    perms.retain(|_id, perm| {
        let age = (now - perm.created_at).num_seconds();
        age < 60
    });
}

/// Truncate a string to at most `max_len` characters, finding a valid UTF-8 char boundary.
fn safe_truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

fn format_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
