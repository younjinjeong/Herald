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

pub const SESSION_COLORS: &[&str] = &["\u{1f7e2}", "\u{1f7e1}", "\u{1f535}", "\u{1f7e3}", "\u{1f7e0}"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub token: SessionToken,
    pub pid: u32,
    pub cwd: PathBuf,
    pub display_name: String,
    pub color_index: usize,
    pub state: SessionState,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    #[serde(default)]
    pub token_usage: TokenUsage,
    #[serde(default)]
    pub conversation_log: Vec<ConversationEntry>,
    #[serde(default)]
    pub tmux_pane: Option<String>,
}

impl SessionInfo {
    /// Format the session tag: "🟢 [project-api]"
    pub fn tag(&self) -> String {
        let color = SESSION_COLORS[self.color_index % SESSION_COLORS.len()];
        format!("{} [{}]", color, self.display_name)
    }

    /// Derive display name from working directory
    pub fn name_from_cwd(cwd: &str) -> String {
        std::path::Path::new(cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("session")
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoDto {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub display_name: String,
    pub color_index: usize,
    pub state: String,
    pub started_at: String,
    pub last_activity: String,
    pub token_usage: TokenUsage,
}

impl SessionInfoDto {
    pub fn tag(&self) -> String {
        let color = SESSION_COLORS[self.color_index % SESSION_COLORS.len()];
        format!("{} [{}]", color, self.display_name)
    }
}

impl From<&SessionInfo> for SessionInfoDto {
    fn from(info: &SessionInfo) -> Self {
        Self {
            id: info.id.0.clone(),
            pid: info.pid,
            cwd: info.cwd.display().to_string(),
            state: format!("{:?}", info.state),
            started_at: info.started_at.to_rfc3339(),
            display_name: info.display_name.clone(),
            color_index: info.color_index,
            last_activity: info.last_activity.to_rfc3339(),
            token_usage: info.token_usage.clone(),
        }
    }
}
