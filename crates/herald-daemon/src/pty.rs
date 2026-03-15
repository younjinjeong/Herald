use anyhow::Result;

/// Inject input into a running process's stdin via /proc (Linux only)
#[cfg(target_os = "linux")]
pub fn inject_input(pid: u32, input: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use tracing::info;

    let fd_path = format!("/proc/{}/fd/0", pid);

    if !is_process_alive(pid) {
        return Err(anyhow::anyhow!("Process {} is not alive", pid));
    }

    info!("Injecting input to PID {} via {}", pid, fd_path);

    let mut file = OpenOptions::new().write(true).open(&fd_path)?;
    writeln!(file, "{}", input)?;

    Ok(())
}

/// PTY injection not available on non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub fn inject_input(_pid: u32, _input: &str) -> Result<()> {
    Err(anyhow::anyhow!(
        "PTY injection is only available on Linux. Use headless mode instead."
    ))
}

#[cfg(target_os = "linux")]
pub fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}/stat", pid)).exists()
}

#[cfg(not(target_os = "linux"))]
pub fn is_process_alive(pid: u32) -> bool {
    // Use kill(pid, 0) to check if process exists
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
