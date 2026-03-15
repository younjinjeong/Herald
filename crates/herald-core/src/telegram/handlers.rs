use teloxide::prelude::*;
use tracing::{info, warn};

use crate::auth::chat_id::{authorize, is_authorized};
use crate::auth::otp::verify_otp;
use crate::telegram::bot::{enqueue_message, BotState};
use crate::telegram::callbacks::parse_callback_data;

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
                    // OTP verified — authorize this chat_id
                    let mut config = state.config.write().await;
                    if let Err(e) = authorize(&mut config, chat_id, &state.config_path) {
                        warn!("Failed to save authorized chat_id: {}", e);
                    }
                    drop(config);
                    *otp_guard = None; // Clear OTP

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
                    *otp_guard = None; // Clear expired/locked OTP
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

    // Authenticated user — forward as prompt
    let active = state.active_session.lock().await;
    if active.is_none() {
        bot.send_message(
            msg.chat.id,
            "No session selected. Use /sessions to pick one, \
             or I'll create a new headless session.",
        )
        .await?;
        // Still forward via headless mode (no specific session)
        enqueue_message(
            &state.queue_tx,
            chat_id,
            format!("Executing: {}", text),
            None,
        )
        .await;
        return Ok(());
    }

    let session_id = active.clone().unwrap();
    drop(active);

    info!("Forwarding prompt to session {}: {}", session_id, text);
    bot.send_message(msg.chat.id, format!("Sending to session {}...", session_id))
        .await?;

    // The daemon's service.rs will handle the actual execution
    // For now, signal via the queue that input was received
    enqueue_message(
        &state.queue_tx,
        chat_id,
        format!("Prompt forwarded to session {}", session_id),
        None,
    )
    .await;

    Ok(())
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
                // Check if session exists
                if let Some(_session) = state.registry.get(payload).await {
                    let mut active = state.active_session.lock().await;
                    *active = Some(payload.to_string());

                    bot.answer_callback_query(&query.id)
                        .text(format!("Session selected: {}", payload))
                        .await?;

                    if let Some(msg) = query.message {
                        bot.send_message(
                            msg.chat().id,
                            format!(
                                "Session {} selected. Send a message to forward it as a prompt.",
                                payload
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
