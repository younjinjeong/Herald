use anyhow::Result;
use tokio::process::Command;
use tracing::{error, info};

pub async fn execute_prompt(prompt: &str) -> Result<String> {
    info!("Executing headless prompt");

    let output = Command::new("claude")
        .arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("text")
        .output()
        .await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        error!("Headless command failed: {}", stderr);
        Err(anyhow::anyhow!("Claude command failed: {}", stderr))
    }
}
