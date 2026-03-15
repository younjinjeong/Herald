use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;

use crate::error::{HeraldError, Result};
use crate::types::{ConversationEntry, SessionInfo, SessionInfoDto, TokenUsage};

const MAX_CONVERSATION_LOG: usize = 50;

#[derive(Debug, Clone)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    color_counter: Arc<std::sync::atomic::AtomicUsize>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            color_counter: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Get next color index for a new session
    pub fn next_color(&self) -> usize {
        self.color_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn register(&self, info: SessionInfo) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(info.id.0.clone(), info);
        Ok(())
    }

    /// Find a session by display_name (for @-prefix routing)
    pub async fn find_by_name(&self, name: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .find(|s| s.display_name == name)
            .cloned()
    }

    /// Get the session tag (color + name) for a session ID
    pub async fn get_tag(&self, id: &str) -> String {
        let sessions = self.sessions.read().await;
        sessions
            .get(id)
            .map(|s| s.tag())
            .unwrap_or_else(|| format!("[{}]", id))
    }

    pub async fn unregister(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions
            .remove(id)
            .ok_or_else(|| HeraldError::Session(format!("Session not found: {}", id)))?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<SessionInfoDto> {
        let sessions = self.sessions.read().await;
        sessions.values().map(SessionInfoDto::from).collect()
    }

    pub async fn update_activity(&self, id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.last_activity = Utc::now();
        }
    }

    /// Update session state (e.g., mark as Stopped)
    pub async fn update_state(&self, id: &str, state: crate::types::SessionState) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.state = state;
            session.last_activity = Utc::now();
        }
    }

    pub async fn count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    pub async fn validate_token(&self, id: &str, token: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions
            .get(id)
            .map(|s| s.token.0 == token)
            .unwrap_or(false)
    }

    /// Update token usage for a session (replaces with latest cumulative values)
    pub async fn update_token_usage(&self, id: &str, usage: TokenUsage) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.token_usage = usage;
            session.last_activity = Utc::now();
        }
    }

    /// Get token usage for a session
    pub async fn get_token_usage(&self, id: &str) -> Option<TokenUsage> {
        let sessions = self.sessions.read().await;
        sessions.get(id).map(|s| s.token_usage.clone())
    }

    /// Get aggregated token usage across all sessions
    pub async fn total_token_usage(&self) -> TokenUsage {
        let sessions = self.sessions.read().await;
        let mut total = TokenUsage::default();
        for session in sessions.values() {
            total.input_tokens += session.token_usage.input_tokens;
            total.output_tokens += session.token_usage.output_tokens;
            total.cache_read_tokens += session.token_usage.cache_read_tokens;
            total.cache_creation_tokens += session.token_usage.cache_creation_tokens;
            total.total_cost_usd += session.token_usage.total_cost_usd;
        }
        total
    }

    /// Add a conversation log entry (ring buffer, max 50 per session)
    pub async fn add_conversation_entry(&self, id: &str, entry: ConversationEntry) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.conversation_log.push(entry);
            if session.conversation_log.len() > MAX_CONVERSATION_LOG {
                session.conversation_log.remove(0);
            }
            session.last_activity = Utc::now();
        }
    }

    /// Get conversation log for a session
    pub async fn get_conversation_log(&self, id: &str) -> Vec<ConversationEntry> {
        let sessions = self.sessions.read().await;
        sessions
            .get(id)
            .map(|s| s.conversation_log.clone())
            .unwrap_or_default()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
