use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info};

pub struct OutboundMessage {
    pub chat_id: i64,
    pub text: String,
    pub parse_mode: Option<String>,
}

pub struct MessageQueue {
    tx: mpsc::Sender<OutboundMessage>,
}

impl MessageQueue {
    pub fn new(rate_limit_per_sec: u32) -> (Self, mpsc::Receiver<OutboundMessage>) {
        let (tx, rx) = mpsc::channel(256);
        (Self { tx }, rx)
    }

    pub async fn enqueue(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        self.tx
            .send(msg)
            .await
            .map_err(|_| anyhow::anyhow!("Message queue closed"))?;
        Ok(())
    }
}

pub async fn drain_queue(
    mut rx: mpsc::Receiver<OutboundMessage>,
    rate_limit_per_sec: u32,
) {
    let interval = Duration::from_millis(1000 / rate_limit_per_sec as u64);

    while let Some(msg) = rx.recv().await {
        // TODO: Send via teloxide bot
        info!("Would send to chat {}: {}", msg.chat_id, msg.text);
        sleep(interval).await;
    }
}
