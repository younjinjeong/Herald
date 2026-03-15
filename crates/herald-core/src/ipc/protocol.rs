use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{HeraldError, Result};
use crate::types::SessionInfoDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcRequest {
    Register {
        session_id: String,
        pid: u32,
        cwd: String,
        #[serde(default)]
        tmux_pane: Option<String>,
    },
    Unregister {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
    },
    Output {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
        tool_name: String,
        tool_input_summary: String,
        tool_response_summary: String,
    },
    Notification {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
        notification_type: String,
        message: String,
        #[serde(default)]
        extras: Option<String>,
    },
    SessionStopped {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
        last_message: String,
    },
    Input {
        session_id: String,
        prompt: String,
    },
    TokenUpdate {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
        total_cost_usd: f64,
    },
    ConversationEntry {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
        entry_type: String,
        content: String,
        timestamp: String,
    },
    PermissionRequest {
        session_id: String,
        #[serde(default)]
        token: Option<String>,
        request_id: String,
        tool_name: String,
        tool_input: String,
    },
    PermissionCheck {
        request_id: String,
    },
    ModeQuery {
        session_id: String,
    },
    ModeUpdate {
        session_id: String,
        plan_mode: bool,
        bypass_permissions: bool,
    },
    Health,
    ListSessions,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcResponse {
    Ok {
        message: Option<String>,
    },
    Registered {
        token: String,
    },
    Error {
        code: u32,
        message: String,
    },
    SessionList {
        sessions: Vec<SessionInfoDto>,
    },
    HealthStatus {
        uptime_secs: u64,
        session_count: usize,
        telegram_connected: bool,
    },
    PermissionResult {
        decision: String,
    },
    ModeResult {
        plan_mode: bool,
        bypass_permissions: bool,
    },
}

/// Write a length-prefixed JSON message to a writer
pub async fn write_message<W: AsyncWriteExt + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<()> {
    let json = serde_json::to_vec(msg)?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-prefixed JSON message from a reader
pub async fn read_message<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 1024 * 1024 {
        return Err(HeraldError::Ipc(format!(
            "Message too large: {} bytes",
            len
        )));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;

    let msg: T = serde_json::from_slice(&buf)?;
    Ok(msg)
}
