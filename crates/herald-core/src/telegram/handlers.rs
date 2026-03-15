use teloxide::prelude::*;
use tracing::{error, info, warn};

use crate::auth::chat_id::{authorize, is_authorized};
use crate::auth::otp::verify_otp;
use crate::ipc::client::IpcClient;
use crate::ipc::protocol::{IpcRequest, IpcResponse};
use crate::telegram::bot::{enqueue_message, BotState};
use crate::telegram::callbacks::{build_session_actions_keyboard, parse_callback_data};
use crate::telegram::formatting::escape_markdown_v2;
use crate::types::SESSION_COLORS;

/// Handle plain text messages (OTP verification or prompt forwarding)
pub async fn text_handler(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let text = match msg.text() {
        Some(t) => t.trim().to_string(),
        None => return Ok(()),
    };

    let chat_id = msg.chat.id.0;
    let config = state.config.read().await;
    let is_auth = is_authorized(&config, chat_id);
    drop(config);

    if !is_auth {
        // Treat as OTP input
        let mut otp_guard = state.pending_otp.lock().await;
        if let Some(ref mut record) = *otp_guard {
            match verify_otp(&text, record) {
                Ok(true) => {
                    let mut config = state.config.write().await;
                    if let Err(e) = authorize(&mut config, chat_id, &state.config_path) {
                        warn!("Failed to save authorized chat_id: {}", e);
                    }
                    drop(config);
                    *otp_guard = None;

                    info!("Chat {} authenticated via OTP", chat_id);
                    bot.send_message(
                        msg.chat.id,
                        "Verified! Herald is now connected.\n\n\
                         Use /sessions to see active Claude Code sessions.\n\
                         Use /status to check daemon status.",
                    )
                    .await?;
                }
                Ok(false) => {
                    bot.send_message(msg.chat.id, "Wrong code. Please try again.")
                        .await?;
                }
                Err(e) => {
                    *otp_guard = None;
                    bot.send_message(msg.chat.id, format!("Authentication failed: {}", e))
                        .await?;
                }
            }
        } else {
            bot.send_message(
                msg.chat.id,
                "No pending verification. Run `herald setup` in your terminal first.",
            )
            .await?;
        }
        return Ok(());
    }

    // === Session routing: determine target session ===
    let target_session = resolve_target_session(&text, &msg, &state).await;

    let (session_id, prompt) = match target_session {
        Some((id, prompt)) => (id, prompt),
        None => {
            bot.send_message(
                msg.chat.id,
                "No session selected. Use /sessions to pick one, or prefix with @session_name.",
            )
            .await?;
            return Ok(());
        }
    };

    // Get session tag for confirmation
    let tag = state.registry.get_tag(&session_id).await;
    info!("Forwarding prompt to session {}: {}", session_id, prompt);

    // Forward the prompt to the session via IPC Input request
    let config = state.config.read().await;
    let transport = crate::ipc::client::IpcTransport::from_config(
        &config.daemon.socket_path,
        &config.daemon.listen_addr,
        &config.daemon.transport,
    );
    drop(config);

    let request = IpcRequest::Input {
        session_id: session_id.clone(),
        prompt: prompt.clone(),
    };

    let queue_tx = state.queue_tx.clone();
    let tag_clone = tag.clone();

    // Show "Delivered" immediately so user knows the message was received
    enqueue_message(
        &state.queue_tx,
        chat_id,
        format!("{} \u{1f4e8} _Delivered_", escape_markdown_v2(&tag)),
        Some("MarkdownV2".to_string()),
    )
    .await;

    // Execute in background — Claude's response will arrive separately
    tokio::spawn(async move {
        match IpcClient::send_via(&transport, &request).await {
            Ok(resp) => {
                if let IpcResponse::Error { code, message } = &resp {
                    error!("Input execution failed ({}): {}", code, message);
                    enqueue_message(
                        &queue_tx,
                        chat_id,
                        format!("{} Failed: {}", escape_markdown_v2(&tag_clone), escape_markdown_v2(message)),
                        Some("MarkdownV2".to_string()),
                    )
                    .await;
                } else {
                    info!("Input forwarded to session: {:?}", resp);
                }
            }
            Err(e) => {
                error!("Failed to forward input: {}", e);
                enqueue_message(
                    &queue_tx,
                    chat_id,
                    format!("{} Failed to forward prompt: {}", tag_clone, e),
                    None,
                )
                .await;
            }
        }
    });

    Ok(())
}

/// Resolve which session to target based on message context:
/// 1. @session_name prefix → route to named session
/// 2. Reply to a tagged message → extract session from tag
/// 3. Active session → use current selection
async fn resolve_target_session(
    text: &str,
    msg: &Message,
    state: &BotState,
) -> Option<(String, String)> {
    // 1. Check for @session_name prefix
    if text.starts_with('@') {
        if let Some(space_pos) = text.find(' ') {
            let name = &text[1..space_pos];
            let prompt = text[space_pos + 1..].trim();
            if let Some(session) = state.registry.find_by_name(name).await {
                return Some((session.id.0, prompt.to_string()));
            }
        }
    }

    // 2. Check if replying to a tagged session message
    if let Some(reply) = &msg.reply_to_message() {
        if let Some(reply_text) = reply.text() {
            if let Some(session_id) = extract_session_from_tag(reply_text, state).await {
                return Some((session_id, text.to_string()));
            }
        }
    }

    // 3. Fall back to active session
    let active = state.active_session.lock().await;
    active.as_ref().map(|id| (id.clone(), text.to_string()))
}

/// Extract session ID from a tagged message like "🟢 [project-api] ..."
async fn extract_session_from_tag(text: &str, state: &BotState) -> Option<String> {
    // Look for pattern: color_emoji [name]
    for color in SESSION_COLORS {
        if text.starts_with(color) {
            // Find [name] part
            if let Some(start) = text.find('[') {
                if let Some(end) = text.find(']') {
                    let name = &text[start + 1..end];
                    if let Some(session) = state.registry.find_by_name(name).await {
                        return Some(session.id.0);
                    }
                }
            }
        }
    }
    None
}

/// Handle inline keyboard callback queries (session selection, approval, etc.)
pub async fn callback_handler(
    bot: Bot,
    query: CallbackQuery,
    state: BotState,
) -> ResponseResult<()> {
    let data = match query.data {
        Some(ref d) => d.as_str(),
        None => return Ok(()),
    };

    if let Some((action, payload)) = parse_callback_data(data) {
        match action {
            "select_session" => {
                if let Some(session) = state.registry.get(payload).await {
                    let mut active = state.active_session.lock().await;
                    *active = Some(payload.to_string());

                    let tag = session.tag();
                    let modes = session.modes.clone();
                    bot.answer_callback_query(&query.id)
                        .text(format!("Active: {}", tag))
                        .await?;

                    if let Some(msg) = query.message {
                        bot.send_message(
                            msg.chat().id,
                            format!(
                                "{} Session selected. Send a message to forward it as a prompt.\n\
                                 Tip: reply to any session message or use @name to target other sessions.",
                                tag
                            ),
                        )
                        .await?;

                        // Show mode toggle buttons
                        let keyboard = build_session_actions_keyboard(payload, &modes);
                        bot.send_message(msg.chat().id, format!("{} Session modes:", tag))
                            .reply_markup(keyboard)
                            .await?;
                    }
                } else {
                    bot.answer_callback_query(&query.id)
                        .text("Session not found")
                        .await?;
                }
            }
            "approve" | "deny" => {
                let request_id = payload;
                let mut perms = state.pending_permissions.lock().await;
                if let Some(perm) = perms.get_mut(request_id) {
                    let decision = action.to_string();
                    perm.decision = Some(decision.clone());
                    drop(perms);

                    let label = if action == "approve" {
                        "\u{2705} Approved"
                    } else {
                        "\u{274c} Denied"
                    };
                    bot.answer_callback_query(&query.id)
                        .text(label)
                        .await?;

                    // Edit the original message to remove buttons and show decision
                    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(msg)) = query.message {
                        let original_text = msg.text().unwrap_or("");
                        let _ = bot
                            .edit_message_text(msg.chat.id, msg.id, format!("{} \\({}\\)", original_text, label))
                            .await;
                    }
                } else {
                    bot.answer_callback_query(&query.id)
                        .text("Request expired or not found")
                        .await?;
                }
            }
            "toggle_plan" | "toggle_bypass" => {
                let session_id = payload;
                if let Some(mut modes) = state.registry.get_modes(session_id).await {
                    if action == "toggle_plan" {
                        modes.plan_mode = !modes.plan_mode;
                        if modes.plan_mode {
                            modes.bypass_permissions = false;
                        }
                    } else {
                        modes.bypass_permissions = !modes.bypass_permissions;
                        if modes.bypass_permissions {
                            modes.plan_mode = false;
                        }
                    }
                    state
                        .registry
                        .update_modes(session_id, modes.clone())
                        .await;

                    let status = if action == "toggle_plan" {
                        format!(
                            "Plan mode: {}",
                            if modes.plan_mode { "ON" } else { "OFF" }
                        )
                    } else {
                        format!(
                            "Bypass permissions: {}",
                            if modes.bypass_permissions {
                                "ON"
                            } else {
                                "OFF"
                            }
                        )
                    };
                    bot.answer_callback_query(&query.id)
                        .text(&status)
                        .await?;

                    // Update the keyboard in-place
                    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(msg)) =
                        query.message
                    {
                        let keyboard =
                            build_session_actions_keyboard(session_id, &modes);
                        let _ = bot
                            .edit_message_reply_markup(msg.chat.id, msg.id)
                            .reply_markup(keyboard)
                            .await;
                    }
                } else {
                    bot.answer_callback_query(&query.id)
                        .text("Session not found")
                        .await?;
                }
            }
            "askq" => {
                // AskUserQuestion response: payload is "request_id:option_index"
                if let Some((request_id, idx_str)) = payload.split_once(':') {
                    let option_idx: usize = idx_str.parse().unwrap_or(0);

                    let pqs = state.pending_questions.lock().await;
                    if let Some(pq) = pqs.get(request_id) {
                        if let Some(option) = pq.options.get(option_idx) {
                            let session_id = pq.session_id.clone();
                            let selected_label = option.label.clone();
                            drop(pqs);

                            bot.answer_callback_query(&query.id)
                                .text(format!("Selected: {}", selected_label))
                                .await?;

                            // Edit message to show selection and remove buttons
                            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(msg)) =
                                query.message
                            {
                                let original_text = msg.text().unwrap_or("");
                                let _ = bot
                                    .edit_message_text(
                                        msg.chat.id,
                                        msg.id,
                                        format!("{}\n\n\u{2705} Selected: {}", original_text, selected_label),
                                    )
                                    .await;

                                // Send the selection to Claude Code via headless continue
                                let prompt = format!("I select: {}", selected_label);
                                let chat_id = msg.chat.id.0;
                                let queue_tx = state.queue_tx.clone();
                                let config = state.config.read().await;
                                let transport = crate::ipc::client::IpcTransport::from_config(
                                    &config.daemon.socket_path,
                                    &config.daemon.listen_addr,
                                    &config.daemon.transport,
                                );
                                drop(config);

                                let request = IpcRequest::Input {
                                    session_id,
                                    prompt,
                                };

                                tokio::spawn(async move {
                                    match IpcClient::send_via(&transport, &request).await {
                                        Ok(_) => {
                                            info!("Question answer delivered to session");
                                        }
                                        Err(e) => {
                                            error!("Failed to deliver question answer: {}", e);
                                            enqueue_message(
                                                &queue_tx,
                                                chat_id,
                                                format!("Failed to deliver selection: {}", e),
                                                None,
                                            )
                                            .await;
                                        }
                                    }
                                });
                            }

                            // Remove from pending
                            let mut pqs = state.pending_questions.lock().await;
                            pqs.remove(request_id);
                        } else {
                            drop(pqs);
                            bot.answer_callback_query(&query.id)
                                .text("Option not found")
                                .await?;
                        }
                    } else {
                        drop(pqs);
                        bot.answer_callback_query(&query.id)
                            .text("Question expired")
                            .await?;
                    }
                }
            }
            _ => {
                bot.answer_callback_query(&query.id)
                    .text("Unknown action")
                    .await?;
            }
        }
    }

    Ok(())
}
