use anyhow::Result;
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use tracing::info;

/// Inject input into a Claude Code session via tmux send-keys
pub async fn inject_via_tmux(pane: &str, input: &str) -> Result<()> {
    info!("Injecting input via tmux send-keys to pane {}", pane);

    // Send the prompt text
    let status = Command::new("tmux")
        .args(["send-keys", "-t", pane, input])
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow::anyhow!("tmux send-keys (text) failed: {}", status));
    }

    // Brief delay to let TUI process the text
    sleep(Duration::from_millis(300)).await;

    // Dismiss autocomplete popup
    let status = Command::new("tmux")
        .args(["send-keys", "-t", pane, "Escape"])
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow::anyhow!("tmux send-keys (Escape) failed: {}", status));
    }

    sleep(Duration::from_millis(100)).await;

    // Submit the prompt
    let status = Command::new("tmux")
        .args(["send-keys", "-t", pane, "Enter"])
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow::anyhow!("tmux send-keys (Enter) failed: {}", status));
    }

    Ok(())
}

/// Capture the current contents of a tmux pane (screen buffer)
pub async fn capture_tmux_pane(pane: &str) -> Result<String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", pane])
        .output()
        .await?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "tmux capture-pane failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string())
}

#[cfg(target_os = "linux")]
pub fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}/stat", pid)).exists()
}

#[cfg(not(target_os = "linux"))]
pub fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
