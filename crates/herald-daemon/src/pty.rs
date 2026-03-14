use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::{info, warn};

pub fn inject_input(pid: u32, input: &str) -> Result<()> {
    let fd_path = format!("/proc/{}/fd/0", pid);

    // Check if process is alive
    let stat_path = format!("/proc/{}/stat", pid);
    if !std::path::Path::new(&stat_path).exists() {
        return Err(anyhow::anyhow!("Process {} is not alive", pid));
    }

    info!("Injecting input to PID {} via {}", pid, fd_path);

    let mut file = OpenOptions::new().write(true).open(&fd_path)?;
    // Write the input followed by a newline
    writeln!(file, "{}", input)?;

    Ok(())
}

pub fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}/stat", pid)).exists()
}
