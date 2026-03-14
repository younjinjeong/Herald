use anyhow::Result;
use herald_core::config::HeraldConfig;
use herald_core::ipc::client::IpcClient;
use herald_core::ipc::protocol::{IpcRequest, IpcResponse};

pub async fn run() -> Result<()> {
    let config = HeraldConfig::load(&HeraldConfig::default_path())?;
    let response = IpcClient::send(&config.daemon.socket_path, &IpcRequest::Health).await?;

    match response {
        IpcResponse::HealthStatus {
            uptime_secs,
            session_count,
            telegram_connected,
        } => {
            let hours = uptime_secs / 3600;
            let minutes = (uptime_secs % 3600) / 60;
            println!("Herald Status");
            println!("=============");
            println!("Uptime:    {}h {}m", hours, minutes);
            println!("Sessions:  {}", session_count);
            println!(
                "Telegram:  {}",
                if telegram_connected {
                    "Connected"
                } else {
                    "Disconnected"
                }
            );
        }
        IpcResponse::Error { message, .. } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response");
        }
    }
    Ok(())
}
