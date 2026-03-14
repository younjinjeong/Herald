use crate::types::SessionInfoDto;

const TELEGRAM_MAX_LENGTH: usize = 4096;

pub fn escape_markdown_v2(text: &str) -> String {
    let special_chars = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];
    let mut result = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if special_chars.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

pub fn truncate_message(text: &str, max_len: usize) -> String {
    let max_len = max_len.min(TELEGRAM_MAX_LENGTH);
    if text.len() <= max_len {
        return text.to_string();
    }
    let suffix = "\n... (truncated)";
    let truncated = &text[..max_len - suffix.len()];
    format!("{}{}", truncated, suffix)
}

pub fn format_session_list(sessions: &[SessionInfoDto]) -> String {
    if sessions.is_empty() {
        return "No active sessions\\.".to_string();
    }
    let mut result = String::from("*Active Sessions:*\n\n");
    for (i, session) in sessions.iter().enumerate() {
        result.push_str(&format!(
            "*{}\\)* `{}`\n  Dir: `{}`\n  State: {}\n  Since: {}\n\n",
            i + 1,
            escape_markdown_v2(&session.id),
            escape_markdown_v2(&session.cwd),
            escape_markdown_v2(&session.state),
            escape_markdown_v2(&session.started_at),
        ));
    }
    result
}

pub fn format_tool_output(tool_name: &str, output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let preview = if lines.len() > max_lines {
        let preview_text: String = lines[..max_lines].join("\n");
        format!(
            "{}\n\\.\\.\\. \\({} more lines\\)",
            preview_text,
            lines.len() - max_lines
        )
    } else {
        output.to_string()
    };

    format!(
        "*Tool:* `{}`\n```\n{}\n```",
        escape_markdown_v2(tool_name),
        preview
    )
}

pub fn format_status(uptime_secs: u64, session_count: usize, telegram_connected: bool) -> String {
    let hours = uptime_secs / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let connected = if telegram_connected { "Connected" } else { "Disconnected" };

    format!(
        "*Herald Status*\n\nUptime: {}h {}m\nSessions: {}\nTelegram: {}",
        hours,
        minutes,
        session_count,
        escape_markdown_v2(connected)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_markdown_v2() {
        assert_eq!(escape_markdown_v2("hello.world"), "hello\\.world");
        assert_eq!(escape_markdown_v2("test_func"), "test\\_func");
        assert_eq!(escape_markdown_v2("normal"), "normal");
    }

    #[test]
    fn test_truncate_message() {
        let short = "hello";
        assert_eq!(truncate_message(short, 100), "hello");

        let long = "a".repeat(200);
        let truncated = truncate_message(&long, 50);
        assert!(truncated.len() <= 50);
        assert!(truncated.ends_with("... (truncated)"));
    }
}
