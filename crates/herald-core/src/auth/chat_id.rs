use std::path::Path;

use crate::config::HeraldConfig;
use crate::error::Result;

pub fn is_authorized(config: &HeraldConfig, chat_id: i64) -> bool {
    config.auth.allowed_chat_ids.contains(&chat_id)
}

pub fn authorize(config: &mut HeraldConfig, chat_id: i64, config_path: &Path) -> Result<()> {
    if !config.auth.allowed_chat_ids.contains(&chat_id) {
        config.auth.allowed_chat_ids.push(chat_id);
        config.save(config_path)?;
    }
    Ok(())
}

pub fn revoke(config: &mut HeraldConfig, chat_id: i64, config_path: &Path) -> Result<()> {
    config.auth.allowed_chat_ids.retain(|&id| id != chat_id);
    config.save(config_path)?;
    Ok(())
}
