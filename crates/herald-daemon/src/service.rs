use anyhow::Result;
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
use herald_core::telegram::formatting::{escape_markdown_v2, format_tool_output};
use herald_core::types::{ConversationEntry, SessionId, SessionInfo, SessionState, TokenUsage};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tracing::{error, info};

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

    let logger_for_ipc = logger.clone();
    let ipc_handle = tokio::spawn(async move {
        let registry = registry_for_ipc;
        let config = config_for_ipc;
        let queue_tx = queue_tx_for_ipc;
        let tc = telegram_connected_for_ipc;
        let start = start_time;
        let shutdown = shutdown_tx_clone;
        let logger = logger_for_ipc;

        if let Err(e) = ipc_server
            .run(move |request| {
                let registry = registry.clone();
                let config = config.clone();
                let queue_tx = queue_tx.clone();
                let tc = tc.clone();
                let shutdown = shutdown.clone();
                let logger = logger.clone();
                async move {
                    handle_request(request, &registry, &config, &queue_tx, &tc, start, &shutdown, &logger)
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
) -> IpcResponse {
    match request {
        IpcRequest::Register {
            session_id,
            pid,
            cwd,
        } => {
            let token = generate_token();
            let display_name = SessionInfo::name_from_cwd(&cwd);
            let color_index = registry.next_color();
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
            };
            let tag = info.tag();
            match registry.register(info).await {
                Ok(()) => {
                    info!("Session registered: {} (PID {})", session_id, pid);
                    logger.log_session_event(&session_id, &format!("started ({})", cwd));
                    let config = config.read().await;
                    let text = format!("{} Session started\nDir: {}", tag, cwd);
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
            tool_input_summary,
            tool_response_summary,
        } => {
            if !registry.validate_token(&session_id, &token).await {
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            registry.update_activity(&session_id).await;
            let tag = registry.get_tag(&session_id).await;

            let config = config.read().await;
            let filtered_response =
                filter_content(&tool_response_summary, &config.output_filter);
            let filtered_input =
                filter_content(&tool_input_summary, &config.output_filter);
            let tool_text = format_tool_output(
                &tool_name,
                &filtered_response,
                config.output_filter.code_preview_lines,
            );
            let text = if filtered_input.is_empty() {
                format!("{}\n{}", tag, tool_text)
            } else {
                format!("{}\n{}\nInput: {}", tag, tool_text, filtered_input)
            };
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
        IpcRequest::Notification {
            session_id,
            token,
            notification_type,
            message,
        } => {
            if !registry.validate_token(&session_id, &token).await {
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
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            let tag = registry.get_tag(&session_id).await;
            let _ = registry.unregister(&session_id).await;

            let config = config.read().await;
            let text = if last_message.is_empty() {
                format!("{} Session stopped.", tag)
            } else {
                let truncated = if last_message.len() > 200 {
                    format!("{}...", &last_message[..200])
                } else {
                    last_message
                };
                format!("{} Session stopped.\nLast: {}", tag, truncated)
            };
            for &chat_id in &config.auth.allowed_chat_ids {
                enqueue_message(queue_tx, chat_id, text.clone(), None).await;
            }
            IpcResponse::Ok { message: None }
        }
        IpcRequest::Input {
            session_id,
            prompt,
        } => {
            // Try PTY injection if session has a live PID, otherwise use headless
            if let Some(session) = registry.get(&session_id).await {
                if crate::pty::is_process_alive(session.pid) {
                    match crate::pty::inject_input(session.pid, &prompt) {
                        Ok(()) => {
                            let tag = registry.get_tag(&session_id).await;
                            let config = config.read().await;
                            for &chat_id in &config.auth.allowed_chat_ids {
                                enqueue_message(
                                    queue_tx, chat_id,
                                    format!("{} Input injected via PTY", tag),
                                    None,
                                ).await;
                            }
                            return IpcResponse::Ok {
                                message: Some("Input injected via PTY".to_string()),
                            };
                        }
                        Err(e) => {
                            tracing::warn!("PTY injection failed, falling back to headless: {}", e);
                        }
                    }
                }
            }

            // Fallback: headless mode
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

            // Send live-updating token message to Telegram
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
                    let config = config.read().await;
                    let text = format!("{} \u{1f464} You: \"{}\"", tag, &content);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), None).await;
                    }
                }
                "assistant_response" => {
                    logger.log_assistant_response(&session_id, &content);
                    let config = config.read().await;
                    let display = if content.len() > 500 {
                        format!("{}...", &content[..500])
                    } else {
                        content.clone()
                    };
                    let text = format!("{} \u{1f916} Claude: {}", tag, display);
                    for &chat_id in &config.auth.allowed_chat_ids {
                        enqueue_message(queue_tx, chat_id, text.clone(), None).await;
                    }
                }
                "tool_summary" => {
                    logger.log_tool_summary(&session_id, &content);
                    // Tool summaries already sent via Output handler, skip duplicate
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

fn format_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
