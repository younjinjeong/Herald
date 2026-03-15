use anyhow::Result;
use std::time::Duration;

use herald_core::config::HeraldConfig;
use herald_core::ipc::client::IpcClient;
use herald_core::ipc::protocol::IpcRequest;

pub async fn run() -> Result<()> {
    let config = HeraldConfig::load(&HeraldConfig::default_path())?;

    // Check if already running
    if config.daemon.socket_path.exists() {
        match IpcClient::send(&config.daemon.socket_path, &IpcRequest::Health).await {
            Ok(_) => {
                println!("Herald daemon is already running.");
                return Ok(());
            }
            Err(_) => {
                // Stale socket, clean up
                let _ = std::fs::remove_file(&config.daemon.socket_path);
            }
        }
    }

    // Spawn heraldd as background process
    println!("Starting Herald daemon...");

    // Log to file so we can debug
    let log_dir = config.daemon.socket_path.parent()
        .unwrap_or(std::path::Path::new("/tmp/herald"));
    let _ = std::fs::create_dir_all(log_dir);
    let log_path = log_dir.join("heraldd.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();

    let stderr_out = match log_file {
        Some(f) => {
            println!("  Log: {}", log_path.display());
            std::process::Stdio::from(f)
        }
        None => std::process::Stdio::null(),
    };

    let child = std::process::Command::new("heraldd")
        .stdout(std::process::Stdio::null())
        .stderr(stderr_out)
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to start heraldd: {}. Is it installed? Try: cargo install --path crates/herald-daemon",
                e
            )
        })?;

    println!("  PID: {}", child.id());

    // Wait for daemon to be ready
    let mut attempts = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Ok(_) =
            IpcClient::send(&config.daemon.socket_path, &IpcRequest::Health).await
        {
            println!("  Herald daemon is ready.");
            return Ok(());
        }
        attempts += 1;
        if attempts > 25 {
            // 5 seconds
            println!("  Warning: daemon started but not yet responding on IPC socket.");
            println!("  Check logs: journalctl --user -u heraldd -f");
            return Ok(());
        }
    }
}
