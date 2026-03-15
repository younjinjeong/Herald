use teloxide::prelude::*;
use tracing::{info, warn};

use crate::auth::chat_id::{authorize, is_authorized};
use crate::auth::otp::verify_otp;
use crate::telegram::bot::{enqueue_message, BotState};
use crate::telegram::callbacks::parse_callback_data;
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
    bot.send_message(msg.chat.id, format!("{} Sending prompt...", tag))
        .await?;

    enqueue_message(
        &state.queue_tx,
        chat_id,
        format!("{} Prompt forwarded", tag),
        None,
    )
    .await;

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

/// Handle inline keyboard callback queries (session selection, etc.)
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
                    }
                } else {
                    bot.answer_callback_query(&query.id)
                        .text("Session not found")
                        .await?;
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
