use crate::types::{SessionInfoDto, TokenUsage};

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

/// Split a message at paragraph boundaries for Telegram's 4096-char limit
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    let max_len = max_len.min(TELEGRAM_MAX_LENGTH);
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut parts = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            parts.push(remaining.to_string());
            break;
        }

        // Try to split at paragraph boundary (double newline)
        let chunk = &remaining[..max_len];
        let split_pos = chunk.rfind("\n\n")
            .or_else(|| chunk.rfind('\n'))
            .unwrap_or(max_len);

        // Ensure we're at a valid char boundary
        let mut pos = split_pos;
        while pos > 0 && !remaining.is_char_boundary(pos) {
            pos -= 1;
        }
        if pos == 0 {
            pos = max_len;
            while pos < remaining.len() && !remaining.is_char_boundary(pos) {
                pos += 1;
            }
        }

        parts.push(remaining[..pos].to_string());
        remaining = remaining[pos..].trim_start_matches('\n');
    }

    parts
}

/// Format session start message in MarkdownV2
pub fn format_session_start(tag: &str, cwd: &str) -> String {
    format!(
        "{} *Session started*\n\u{1f4c1} `{}`",
        escape_markdown_v2(tag),
        escape_markdown_v2(cwd),
    )
}

/// Format "working on" message in MarkdownV2
pub fn format_working(tag: &str, prompt_preview: &str) -> String {
    format!(
        "{} \u{1f528} Working on:\n> {}",
        escape_markdown_v2(tag),
        escape_markdown_v2(prompt_preview),
    )
}

/// Format completion/done message in MarkdownV2
pub fn format_completion(tag: &str, tool_count: u32, usage: Option<&TokenUsage>, response: &str) -> String {
    let separator = "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}";

    let mut text = if tool_count > 0 {
        format!("{} \u{2705} Done \\({} tools\\)\n{}", escape_markdown_v2(tag), tool_count, separator)
    } else {
        format!("{} \u{1f4ac} Responded\n{}", escape_markdown_v2(tag), separator)
    };

    if let Some(usage) = usage {
        if usage.input_tokens > 0 || usage.output_tokens > 0 {
            text.push_str(&format!(
                "\n\u{1f4ca} {} in / {} out \u{00b7} ${}\n{}",
                escape_markdown_v2(&format_num(usage.input_tokens)),
                escape_markdown_v2(&format_num(usage.output_tokens)),
                escape_markdown_v2(&format!("{:.4}", usage.total_cost_usd)),
                separator,
            ));
        }
    }

    if !response.is_empty() {
        text.push('\n');
        text.push_str(&escape_markdown_v2(response));
    }

    text
}

/// Format session end message in MarkdownV2
pub fn format_session_end(tag: &str) -> String {
    format!("{} *Session ended*", escape_markdown_v2(tag))
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

fn format_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Format permission request message in MarkdownV2
pub fn format_permission_request(tag: &str, tool_name: &str, tool_input: &str) -> String {
    // Show a compact preview of tool_input
    let input_preview = if tool_input.len() > 200 {
        let mut end = 200;
        while end > 0 && !tool_input.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &tool_input[..end])
    } else {
        tool_input.to_string()
    };

    format!(
        "{} \u{1f512} *Permission request*\n\u{2699}\u{fe0f} Tool: `{}`\n```\n{}\n```",
        escape_markdown_v2(tag),
        escape_markdown_v2(tool_name),
        input_preview,
    )
}

/// Format AskUserQuestion notification in MarkdownV2
pub fn format_ask_user_question(tag: &str, question: &str) -> String {
    format!(
        "{} \u{2753} *Question for user*\n> {}",
        escape_markdown_v2(tag),
        escape_markdown_v2(question),
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

    #[test]
    fn test_split_message_short() {
        let short = "hello world";
        let parts = split_message(short, 4096);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], "hello world");
    }

    #[test]
    fn test_split_message_long() {
        let paragraph1 = "a".repeat(100);
        let paragraph2 = "b".repeat(100);
        let text = format!("{}\n\n{}", paragraph1, paragraph2);
        let parts = split_message(&text, 150);
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn test_format_completion() {
        let usage = TokenUsage {
            input_tokens: 12345,
            output_tokens: 8100,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_cost_usd: 0.0234,
        };
        let text = format_completion("\u{1f7e2} [test]", 5, Some(&usage), "Done!");
        assert!(text.contains("Done"));
        assert!(text.contains("5 tools"));
        assert!(text.contains("12\\.3K"));
    }
}
