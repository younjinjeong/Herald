use anyhow::Result;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

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
pub async fn execute_prompt(prompt: &str, session_id: Option<&str>, cwd: Option<&str>) -> Result<String> {
    let claude_path = resolve_claude_path();
    info!("Executing headless prompt (continue={:?}, cwd={:?}, claude={})", session_id.is_some(), cwd, claude_path);

    let mut cmd = Command::new(&claude_path);

    // Run in the session's working directory so --continue finds the right conversation
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    // Selectively suppress session lifecycle hooks (SessionStart/SessionEnd) while
    // allowing tool activity hooks (PostToolUse, UserPromptSubmit, Stop) to fire.
    // The original session ID is passed so events are attributed to the right session.
    if let Some(sid) = session_id {
        cmd.env("HERALD_HEADLESS_SESSION", sid);
    }

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

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    info!("Headless exit_code={}, stdout_len={}, stderr_len={}", output.status, stdout.len(), stderr.len());
    debug!("Headless stdout (first 500): {}", &stdout[..stdout.len().min(500)]);
    if !stderr.is_empty() {
        warn!("Headless stderr: {}", &stderr[..stderr.len().min(200)]);
    }

    if output.status.success() {
        // If JSON format, extract the result text
        if session_id.is_some() {
            // Try parsing entire stdout as JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
                    info!("Extracted result field ({} chars)", result.len());
                    return Ok(result.to_string());
                }
                warn!("JSON parsed but no 'result' field found. Keys: {:?}",
                    json.as_object().map(|o| o.keys().collect::<Vec<_>>()));
            } else {
                // Maybe stdout has multiple lines; try the last line as JSON
                if let Some(last_line) = stdout.lines().rev().find(|l| l.starts_with('{')) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(last_line) {
                        if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
                            info!("Extracted result from last JSON line ({} chars)", result.len());
                            return Ok(result.to_string());
                        }
                    }
                }
                warn!("Failed to parse stdout as JSON, using raw output");
            }
        }

        Ok(stdout)
    } else {
        error!("Headless command failed ({}): {}", output.status, stderr);
        Err(anyhow::anyhow!("Claude command failed: {}", stderr))
    }
}
