use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use teloxide::dispatching::UpdateFilterExt;
use teloxide::prelude::*;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info};

use crate::auth::otp::OtpRecord;
use crate::config::HeraldConfig;
use crate::session::registry::SessionRegistry;

use super::commands::{command_handler, HeraldCommand};
use super::handlers::{callback_handler, text_handler};

/// Outbound message to send to Telegram
pub struct OutboundMessage {
    pub chat_id: i64,
    pub text: String,
    pub parse_mode: Option<String>,
}

/// Shared state injected into all teloxide handlers
#[derive(Clone)]
pub struct BotState {
    pub config: Arc<RwLock<HeraldConfig>>,
    pub config_path: PathBuf,
    pub registry: SessionRegistry,
    pub queue_tx: mpsc::Sender<OutboundMessage>,
    pub telegram_connected: Arc<AtomicBool>,
    pub start_time: DateTime<Utc>,
    pub pending_otp: Arc<Mutex<Option<OtpRecord>>>,
    pub active_session: Arc<Mutex<Option<String>>>,
}

pub fn create_bot(token: &str) -> Bot {
    Bot::new(token)
}

/// Run the Telegram bot with long polling
pub async fn run_bot(bot: Bot, state: BotState) {
    info!("Starting Telegram bot polling...");

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<HeraldCommand>()
                .endpoint(command_handler),
        )
        .branch(Update::filter_message().endpoint(text_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    state.telegram_connected.store(true, Ordering::Relaxed);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state.clone()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    state.telegram_connected.store(false, Ordering::Relaxed);
    info!("Telegram bot polling stopped");
}

/// Send a message through the outbound queue
pub async fn enqueue_message(
    queue_tx: &mpsc::Sender<OutboundMessage>,
    chat_id: i64,
    text: String,
    parse_mode: Option<String>,
) {
    let msg = OutboundMessage {
        chat_id,
        text,
        parse_mode,
    };
    if let Err(e) = queue_tx.send(msg).await {
        error!("Failed to enqueue message: {}", e);
    }
}

/// Drain the outbound queue and send messages via Telegram
pub async fn drain_queue(mut rx: mpsc::Receiver<OutboundMessage>, bot: Bot) {
    use std::time::Duration;
    use teloxide::types::ParseMode;
    use tokio::time::sleep;

    while let Some(msg) = rx.recv().await {
        let chat_id = ChatId(msg.chat_id);
        let mut request = bot.send_message(chat_id, &msg.text);

        if let Some(ref mode) = msg.parse_mode {
            if mode == "MarkdownV2" {
                request = request.parse_mode(ParseMode::MarkdownV2);
            }
        }

        match request.await {
            Ok(_) => {}
            Err(teloxide::RequestError::RetryAfter(secs)) => {
                let wait = secs.duration();
                tracing::warn!("Rate limited, retrying after {:?}", wait);
                sleep(wait).await;
                // Retry once
                let mut retry = bot.send_message(chat_id, &msg.text);
                if let Some(ref mode) = msg.parse_mode {
                    if mode == "MarkdownV2" {
                        retry = retry.parse_mode(ParseMode::MarkdownV2);
                    }
                }
                if let Err(e) = retry.await {
                    error!("Retry failed: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to send Telegram message: {}", e);
            }
        }

        // Basic rate limiting: ~30 msgs/sec
        sleep(Duration::from_millis(35)).await;
    }
}
