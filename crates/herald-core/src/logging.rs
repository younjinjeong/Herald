use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use chrono::Utc;
use tracing::error;

use crate::types::TokenUsage;

const DEFAULT_LOG_PATH: &str = "/var/log/herald-relay.log";

pub struct ConversationLogger {
    log_path: String,
}

impl ConversationLogger {
    pub fn new(log_path: Option<&str>) -> Self {
        Self {
            log_path: log_path.unwrap_or(DEFAULT_LOG_PATH).to_string(),
        }
    }

    fn write_line(&self, line: &str) {
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            Ok(mut file) => {
                if let Err(e) = writeln!(file, "{}", line) {
                    error!("Failed to write to conversation log: {}", e);
                }
            }
            Err(e) => {
                // Fallback: try user-level path if /var/log is not writable
                let fallback = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join("herald")
                    .join("herald-relay.log");

                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&fallback)
                {
                    let _ = writeln!(file, "{}", line);
                } else {
                    error!(
                        "Failed to open conversation log at {} or {}: {}",
                        self.log_path,
                        fallback.display(),
                        e
                    );
                }
            }
        }
    }

    pub fn log_user_prompt(&self, session_id: &str, content: &str) {
        let ts = Utc::now().to_rfc3339();
        self.write_line(&format!(
            "{} [session:{}] USER: {}",
            ts,
            session_id,
            content.replace('\n', " ")
        ));
    }

    pub fn log_assistant_response(&self, session_id: &str, content: &str) {
        let ts = Utc::now().to_rfc3339();
        // Strip code blocks and keep only descriptive text
        let filtered = filter_assistant_text(content);
        if !filtered.is_empty() {
            self.write_line(&format!(
                "{} [session:{}] CLAUDE: {}",
                ts,
                session_id,
                filtered.replace('\n', " ")
            ));
        }
    }

    pub fn log_tool_summary(&self, session_id: &str, content: &str) {
        let ts = Utc::now().to_rfc3339();
        self.write_line(&format!(
            "{} [session:{}] TOOL: {}",
            ts,
            session_id,
            content.replace('\n', " ")
        ));
    }

    pub fn log_token_usage(&self, session_id: &str, usage: &TokenUsage) {
        let ts = Utc::now().to_rfc3339();
        self.write_line(&format!(
            "{} [session:{}] TOKENS: in={} out={} cache_read={} cache_create={} cost=${:.4}",
            ts,
            session_id,
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_creation_tokens,
            usage.total_cost_usd,
        ));
    }

    pub fn log_session_event(&self, session_id: &str, event: &str) {
        let ts = Utc::now().to_rfc3339();
        self.write_line(&format!(
            "{} [session:{}] EVENT: {}",
            ts, session_id, event
        ));
    }
}

/// Filter assistant text to keep only descriptive content
/// Strips code blocks, command outputs, and keeps prose
fn filter_assistant_text(text: &str) -> String {
    let mut result = Vec::new();
    let mut in_code_block = false;

    for line in text.lines() {
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        // Skip command output lines
        if line.starts_with("$ ") || line.starts_with("> ") {
            continue;
        }
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            result.push(trimmed);
        }
    }

    result.join(" ")
}

impl Default for ConversationLogger {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_assistant_text() {
        let input = "I found the bug.\n\n```rust\nfn main() {}\n```\n\nThe fix is applied.";
        let result = filter_assistant_text(input);
        assert_eq!(result, "I found the bug. The fix is applied.");
    }

    #[test]
    fn test_filter_strips_command_output() {
        let input = "Running tests:\n$ cargo test\n> output line\nAll passed.";
        let result = filter_assistant_text(input);
        assert_eq!(result, "Running tests: All passed.");
    }
}
