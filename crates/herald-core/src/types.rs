use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToken(pub String);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChatId(pub i64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Idle,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationEntry {
    pub timestamp: DateTime<Utc>,
    pub entry_type: String, // "user_prompt" | "assistant_response" | "tool_summary"
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub token: SessionToken,
    pub pid: u32,
    pub cwd: PathBuf,
    pub state: SessionState,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    #[serde(default)]
    pub token_usage: TokenUsage,
    #[serde(default)]
    pub conversation_log: Vec<ConversationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoDto {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub state: String,
    pub started_at: String,
    pub last_activity: String,
    pub token_usage: TokenUsage,
}

impl From<&SessionInfo> for SessionInfoDto {
    fn from(info: &SessionInfo) -> Self {
        Self {
            id: info.id.0.clone(),
            pid: info.pid,
            cwd: info.cwd.display().to_string(),
            state: format!("{:?}", info.state),
            started_at: info.started_at.to_rfc3339(),
            last_activity: info.last_activity.to_rfc3339(),
            token_usage: info.token_usage.clone(),
        }
    }
}
