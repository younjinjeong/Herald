use regex::Regex;

use crate::config::OutputFilterConfig;

pub fn filter_content(text: &str, config: &OutputFilterConfig) -> String {
    let mut filtered = text.to_string();

    if config.mask_secrets {
        for pattern in &config.secret_patterns {
            if let Ok(re) = Regex::new(pattern) {
                filtered = re.replace_all(&filtered, "[REDACTED]").to_string();
            }
        }
    }

    if filtered.len() > config.max_message_length {
        let suffix = "\n... (truncated)";
        filtered = format!(
            "{}{}",
            &filtered[..config.max_message_length - suffix.len()],
            suffix
        );
    }

    filtered
}

pub fn summarize_diff(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let additions = lines.iter().filter(|l| l.starts_with('+')).count();
    let deletions = lines.iter().filter(|l| l.starts_with('-')).count();
    format!("+{} -{} lines", additions, deletions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_secrets() {
        let config = OutputFilterConfig::default();
        let text = "API_KEY=sk-12345abc password: hunter2";
        let filtered = filter_content(text, &config);
        assert!(!filtered.contains("sk-12345abc"));
        assert!(!filtered.contains("hunter2"));
    }

    #[test]
    fn test_truncate_long_content() {
        let config = OutputFilterConfig {
            max_message_length: 50,
            ..Default::default()
        };
        let long_text = "a".repeat(200);
        let filtered = filter_content(&long_text, &config);
        assert!(filtered.len() <= 50);
    }
}
