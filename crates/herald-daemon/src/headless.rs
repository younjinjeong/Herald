use anyhow::Result;
use tokio::process::Command;
use tracing::{error, info};

/// Execute a headless prompt, optionally continuing an existing session.
/// Uses JSON output format when continuing a session for richer data extraction.
pub async fn execute_prompt(prompt: &str, session_id: Option<&str>) -> Result<String> {
    info!("Executing headless prompt (continue={:?})", session_id.is_some());

    let mut cmd = Command::new("claude");

    // Prevent headless process from firing Herald hooks that would
    // overwrite and then destroy the original interactive session's registration
    cmd.env("HERALD_HEADLESS", "1");

    if let Some(sid) = session_id {
        // Continue existing session with JSON output for richer parsing
        cmd.arg("--continue")
            .arg(sid)
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
