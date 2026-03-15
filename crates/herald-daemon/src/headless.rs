use anyhow::Result;
use tokio::process::Command;
use tracing::{error, info};

/// Resolve the full path to the `claude` binary.
/// Checks PATH first, then common install locations.
fn resolve_claude_path() -> String {
    // 1. Check if claude is in PATH
    if let Ok(output) = std::process::Command::new("which")
        .arg("claude")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }

    // 2. Check common install locations
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let candidates = [
        format!("{}/.local/bin/claude", home),
        format!("{}/.cargo/bin/claude", home),
        "/usr/local/bin/claude".to_string(),
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.clone();
        }
    }

    // Fallback to bare command (let OS resolve)
    "claude".to_string()
}

/// Execute a headless prompt, optionally continuing an existing session.
/// Uses JSON output format when continuing a session for richer data extraction.
pub async fn execute_prompt(prompt: &str, session_id: Option<&str>) -> Result<String> {
    let claude_path = resolve_claude_path();
    info!("Executing headless prompt (continue={:?}, claude={})", session_id.is_some(), claude_path);

    let mut cmd = Command::new(&claude_path);

    // Prevent headless process from firing Herald hooks that would
    // overwrite and then destroy the original interactive session's registration
    cmd.env("HERALD_HEADLESS", "1");

    // Skip Claude Code's built-in permission prompts — the Telegram user is
    // OTP-authenticated and explicitly sending this prompt
    cmd.arg("--dangerously-skip-permissions");

    if let Some(_sid) = session_id {
        // Continue the most recent conversation with JSON output for richer parsing
        // Note: --continue targets the latest conversation; --resume only works for
        // inactive sessions and fails on currently active ones.
        cmd.arg("--continue")
            .arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("json");
    } else {
        // New session with text output
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("text");
    }

    let output = cmd.output().await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        // If JSON format, extract the result text
        if session_id.is_some() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                // Extract result field from JSON response
                if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
                    return Ok(result.to_string());
                }
            }
            // Fall back to raw output if JSON parsing fails
        }

        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        error!("Headless command failed: {}", stderr);
        Err(anyhow::anyhow!("Claude command failed: {}", stderr))
    }
}
