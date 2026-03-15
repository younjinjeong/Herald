use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use herald_core::config::HeraldConfig;
use herald_core::ipc::protocol::{IpcRequest, IpcResponse};
use herald_core::ipc::server::IpcServer;
use herald_core::logging::ConversationLogger;
use herald_core::security::content_filter::filter_content;
use herald_core::session::registry::SessionRegistry;
use herald_core::session::token::generate_token;
use herald_core::telegram::bot::{
    create_bot, drain_queue, enqueue_message, run_bot, BotState, OutboundMessage,
};
use herald_core::telegram::formatting::escape_markdown_v2;
use herald_core::types::{ConversationEntry, SessionId, SessionInfo, SessionState, TokenUsage};
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

const DEBOUNCE_SECS: u64 = 15;
const MAX_TELEGRAM_LEN: usize = 3900;

pub async fn run(config: HeraldConfig) -> Result<()> {
    let start_time = chrono::Utc::now();
    let config_path = HeraldConfig::default_path();
    let registry = SessionRegistry::new();
    let telegram_connected = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // Create conversation logger with configured output mode
    let log_output = herald_core::logging::LogOutput::from_str(&config.daemon.log_output);
    let logger = Arc::new(ConversationLogger::new(None, log_output));

    // Create message queue
    let (queue_tx, queue_rx) = mpsc::channel::<OutboundMessage>(256);

    // Create bot and shared state
    let bot_token = config.get_bot_token()?;
    let bot = create_bot(&bot_token);

    let config_arc = Arc::new(RwLock::new(config));

    let bot_state = BotState {
        config: config_arc.clone(),
        config_path: config_path.clone(),
        registry: registry.clone(),
        queue_tx: queue_tx.clone(),
        telegram_connected: telegram_connected.clone(),
        start_time,
        pending_otp: Arc::new(Mutex::new(None)),
        active_session: Arc::new(Mutex::new(None)),
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
    let ipc_handle = tokio::spawn(async move {
        let registry = registry_for_ipc;
        let config = config_for_ipc;
        let queue_tx = queue_tx_for_ipc;
        let tc = telegram_connected_for_ipc;
        let start = start_time;
        let shutdown = shutdown_tx_clone;
        let logger = logger_for_ipc;
        let activity = activity_for_ipc;

        if let Err(e) = ipc_server
            .run(move |request| {
                let registry = registry.clone();
                let config = config.clone();
                let queue_tx = queue_tx.clone();
                let tc = tc.clone();
                let shutdown = shutdown.clone();
                let logger = logger.clone();
                let activity = activity.clone();
                async move {
                    handle_request(request, &registry, &config, &queue_tx, &tc, start, &shutdown, &logger, &activity)
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

    Ok(())
}

async fn handle_request(
    request: IpcRequest,
    registry: &SessionRegistry,
    config: &Arc<RwLock<HeraldConfig>>,
    queue_tx: &mpsc::Sender<OutboundMessage>,
    telegram_connected: &Arc<AtomicBool>,
    start_time: chrono::DateTime<chrono::Utc>,
    shutdown_tx: &watch::Sender<bool>,
    logger: &ConversationLogger,
    activity: &ActivityTracker,
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
            };
            let tag = info.tag();
            match registry.register(info).await {
                Ok(()) => {
                    info!("Session registered: {} (PID {}, tmux={})", session_id, pid, has_tmux);
                    logger.log_session_event(&session_id, &format!("started ({})", cwd));
                    let config = config.read().await;
                    let tmux_status = if has_tmux {
                        ""
                    } else {
                        "\n\u{26a0}\u{fe0f} Not in tmux \u{2014} Telegram input disabled"
                    };
                    let text = format!("{} Session started\nDir: {}{}", tag, cwd, tmux_status);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), None).await;
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
            if !registry.validate_token(&session_id, &token).await {
                warn!("Unregister: invalid token for session {}", session_id);
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            let tag = registry.get_tag(&session_id).await;
            match registry.unregister(&session_id).await {
                Ok(()) => {
                    info!("Session unregistered: {}", session_id);
                    logger.log_session_event(&session_id, "ended");
                    let config = config.read().await;
                    let text = format!("{} Session ended", tag);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), None).await;
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
            tool_input_summary: _,
            tool_response_summary: _,
        } => {
            if !registry.validate_token(&session_id, &token).await {
                warn!("Output: invalid token for session {} (tool: {})", session_id, tool_name);
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            registry.update_activity(&session_id).await;

            // Increment tool count and reset debounce timer (suppress output to Telegram)
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
                tokio::time::sleep(std::time::Duration::from_secs(DEBOUNCE_SECS)).await;
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
        } => {
            if !registry.validate_token(&session_id, &token).await {
                warn!("Notification: invalid token for session {}", session_id);
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }

            let tag = registry.get_tag(&session_id).await;
            let config = config.read().await;
            let text = format!(
                "{} Notification [{}]:\n{}",
                tag,
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
            IpcResponse::Ok { message: None }
        }
        IpcRequest::SessionStopped {
            session_id,
            token,
            last_message,
        } => {
            if !registry.validate_token(&session_id, &token).await {
                warn!("SessionStopped: invalid token for session {}", session_id);
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
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

            // Get token usage before unregistering
            let token_line = if let Some(usage) = registry.get_token_usage(&session_id).await {
                format!(
                    "Tokens: {} in / {} out \u{00b7} ${:.4}",
                    format_num(usage.input_tokens),
                    format_num(usage.output_tokens),
                    usage.total_cost_usd,
                )
            } else {
                String::new()
            };

            let _ = registry.unregister(&session_id).await;

            // Build rich completion message
            let separator = "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}";
            let (tool_count, stored_response) = activity_info.unwrap_or((0, None));

            // Use stored assistant_response, fall back to last_message from hook
            let response_content = stored_response
                .filter(|s| !s.is_empty())
                .unwrap_or(last_message);

            let mut text = if tool_count > 0 {
                format!("{} \u{2705} Done ({} tools used)\n{}", tag, tool_count, separator)
            } else {
                format!("{} \u{1f6d1} Session ended\n{}", tag, separator)
            };
            if !token_line.is_empty() {
                text.push_str(&format!("\n{}\n{}", token_line, separator));
            }
            if !response_content.is_empty() {
                let truncated = safe_truncate(&response_content, MAX_TELEGRAM_LEN);
                text.push('\n');
                text.push_str(&truncated);
            }

            let config = config.read().await;
            for &chat_id in &config.auth.allowed_chat_ids {
                enqueue_message(queue_tx, chat_id, text.clone(), None).await;
            }
            IpcResponse::Ok { message: None }
        }
        IpcRequest::Input {
            session_id,
            prompt,
        } => {
            // Try tmux injection if session has a tmux pane and live PID
            if let Some(session) = registry.get(&session_id).await {
                if let Some(ref pane) = session.tmux_pane {
                    if crate::pty::is_process_alive(session.pid) {
                        match crate::pty::inject_via_tmux(pane, &prompt).await {
                            Ok(()) => {
                                let tag = registry.get_tag(&session_id).await;
                                let config = config.read().await;
                                for &chat_id in &config.auth.allowed_chat_ids {
                                    enqueue_message(
                                        queue_tx, chat_id,
                                        format!("{} Input injected via tmux", tag),
                                        None,
                                    ).await;
                                }
                                return IpcResponse::Ok {
                                    message: Some("Input injected via tmux".to_string()),
                                };
                            }
                            Err(e) => {
                                tracing::warn!("tmux injection failed, falling back to headless: {}", e);
                            }
                        }
                    }
                }
            }

            // Fallback: headless mode (no tmux pane or injection failed)
            let config = config.read().await;
            match crate::headless::execute_prompt(&prompt).await {
                Ok(output) => {
                    let filtered = filter_content(&output, &config.output_filter);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, filtered.clone(), None).await;
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
            if !registry.validate_token(&session_id, &token).await {
                warn!("TokenUpdate: invalid token for session {}", session_id);
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
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
                     Tokens: {} in / {} out\n\
                     Cache:  {} read / {} created\n\
                     Cost:   ${:.4}\n\
                     Updated: {}",
                    tag,
                    format_num(input_tokens),
                    format_num(output_tokens),
                    format_num(cache_read_tokens),
                    format_num(cache_creation_tokens),
                    total_cost_usd,
                    chrono::Utc::now().format("%H:%M:%S"),
                );
                for &chat_id in &config.auth.allowed_chat_ids {
                    enqueue_message(queue_tx, chat_id, text.clone(), None).await;
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
            if !registry.validate_token(&session_id, &token).await {
                warn!("ConversationEntry({}): invalid token for session {}", entry_type, session_id);
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
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
                    let text = format!("{} \u{1f528} Working on: \"{}\"", tag, {
                        let act = activity.lock().await;
                        act.get(&session_id).map(|s| s.prompt_preview.clone()).unwrap_or_default()
                    });
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), None).await;
                    }
                }
                "assistant_response" => {
                    logger.log_assistant_response(&session_id, &content);
                    // Store for use by fire_completion/SessionStopped as fallback
                    let mut act = activity.lock().await;
                    if let Some(state) = act.get_mut(&session_id) {
                        state.last_assistant_response = Some(content.clone());
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
    let token_line = if let Some(usage) = registry.get_token_usage(session_id).await {
        format!(
            "Tokens: {} in / {} out \u{00b7} ${:.4}",
            format_num(usage.input_tokens),
            format_num(usage.output_tokens),
            usage.total_cost_usd,
        )
    } else {
        String::new()
    };

    // Try tmux capture first, fall back to stored assistant_response
    let response_content = if let Some(session) = registry.get(session_id).await {
        if let Some(ref pane) = session.tmux_pane {
            match crate::pty::capture_tmux_pane(pane).await {
                Ok(content) if !content.is_empty() => {
                    safe_truncate_tail(&content, MAX_TELEGRAM_LEN)
                }
                Ok(_) | Err(_) => {
                    // tmux capture empty or failed — use stored response
                    stored_response.unwrap_or_default()
                }
            }
        } else {
            // No tmux pane — use stored response
            stored_response.unwrap_or_default()
        }
    } else {
        stored_response.unwrap_or_default()
    };

    let separator = "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}";
    let mut text = format!(
        "{} \u{2705} Done ({} tools used)\n{}",
        tag, tool_count, separator,
    );
    if !token_line.is_empty() {
        text.push_str(&format!("\n{}\n{}", token_line, separator));
    }
    if !response_content.is_empty() {
        text.push('\n');
        text.push_str(&safe_truncate(&response_content, MAX_TELEGRAM_LEN));
    }

    let config = config.read().await;
    for &chat_id in &config.auth.allowed_chat_ids {
        enqueue_message(queue_tx, chat_id, text.clone(), None).await;
    }
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

/// Take the last `max_len` characters of a string (for tmux captures where recent output is at bottom).
fn safe_truncate_tail(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let mut start = s.len() - max_len;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    format!("...{}", &s[start..])
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
