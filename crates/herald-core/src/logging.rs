use std::fs::OpenOptions;
use std::io::Write;

use chrono::Utc;
use tracing::error;

use crate::types::TokenUsage;

/// Platform-aware default log path
fn default_log_path() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home
                .join("Library/Logs/herald/herald-relay.log")
                .display()
                .to_string();
        }
    }
    "/var/log/herald-relay.log".to_string()
}

/// Log output mode
#[derive(Debug, Clone, PartialEq)]
pub enum LogOutput {
    File,
    Stdout,
    Both,
}

impl LogOutput {
    pub fn from_str(s: &str) -> Self {
        match s {
            "stdout" => Self::Stdout,
            "both" => Self::Both,
            _ => Self::File,
        }
    }
}

pub struct ConversationLogger {
    log_path: String,
    output: LogOutput,
}

impl ConversationLogger {
    pub fn new(log_path: Option<&str>, output: LogOutput) -> Self {
        Self {
            log_path: log_path
                .map(|s| s.to_string())
                .unwrap_or_else(default_log_path),
            output,
        }
    }

    /// Write a structured log entry to configured outputs (file, stdout JSON, or both)
    fn write_entry(&self, session_id: &str, log_type: &str, file_line: &str, content: &str) {
        match self.output {
            LogOutput::File => {
                self.write_to_file(file_line);
            }
            LogOutput::Stdout => {
                self.write_json_stdout(session_id, log_type, content);
            }
            LogOutput::Both => {
                self.write_json_stdout(session_id, log_type, content);
                self.write_to_file(file_line);
            }
        }
    }

    fn write_json_stdout(&self, session_id: &str, log_type: &str, content: &str) {
        let json = serde_json::json!({
            "ts": Utc::now().to_rfc3339(),
            "session": session_id,
            "type": log_type,
            "content": content,
        });
        println!("{}", json);
    }

    fn write_to_file(&self, line: &str) {
        if let Some(parent) = std::path::Path::new(&self.log_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

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
                let fallback = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join("herald")
                    .join("herald-relay.log");

                let _ = std::fs::create_dir_all(fallback.parent().unwrap());
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
        let file_line = format!(
            "{} [session:{}] USER: {}",
            ts, session_id,
            content.replace('\n', " ")
        );
        self.write_entry(session_id, "user_prompt", &file_line, content);
    }

    pub fn log_assistant_response(&self, session_id: &str, content: &str) {
        let filtered = filter_assistant_text(content);
        if filtered.is_empty() {
            return;
        }
        let ts = Utc::now().to_rfc3339();
        let file_line = format!(
            "{} [session:{}] CLAUDE: {}",
            ts, session_id,
            filtered.replace('\n', " ")
        );
        self.write_entry(session_id, "assistant_response", &file_line, &filtered);
    }

    pub fn log_tool_summary(&self, session_id: &str, content: &str) {
        let ts = Utc::now().to_rfc3339();
        let file_line = format!(
            "{} [session:{}] TOOL: {}",
            ts, session_id,
            content.replace('\n', " ")
        );
        self.write_entry(session_id, "tool_summary", &file_line, content);
    }

    pub fn log_token_usage(&self, session_id: &str, usage: &TokenUsage) {
        let ts = Utc::now().to_rfc3339();
        let summary = format!(
            "in={} out={} cache_read={} cache_create={} cost=${:.4}",
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_creation_tokens,
            usage.total_cost_usd,
        );
        let file_line = format!("{} [session:{}] TOKENS: {}", ts, session_id, summary);
        self.write_entry(session_id, "tokens", &file_line, &summary);
    }

    pub fn log_session_event(&self, session_id: &str, event: &str) {
        let ts = Utc::now().to_rfc3339();
        let file_line = format!("{} [session:{}] EVENT: {}", ts, session_id, event);
        self.write_entry(session_id, "event", &file_line, event);
    }
}

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
        Self::new(None, LogOutput::File)
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

    #[test]
    fn test_log_output_from_str() {
        assert_eq!(LogOutput::from_str("file"), LogOutput::File);
        assert_eq!(LogOutput::from_str("stdout"), LogOutput::Stdout);
        assert_eq!(LogOutput::from_str("both"), LogOutput::Both);
        assert_eq!(LogOutput::from_str("unknown"), LogOutput::File);
    }
}
