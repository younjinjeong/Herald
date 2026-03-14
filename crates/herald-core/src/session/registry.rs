use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;

use crate::error::{HeraldError, Result};
use crate::types::{SessionInfo, SessionInfoDto};

#[derive(Debug, Clone)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, info: SessionInfo) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(info.id.0.clone(), info);
        Ok(())
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
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
