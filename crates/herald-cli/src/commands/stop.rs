use anyhow::Result;
use herald_core::config::HeraldConfig;
use herald_core::ipc::client::IpcClient;
use herald_core::ipc::protocol::IpcRequest;

pub async fn run() -> Result<()> {
    let config = HeraldConfig::load(&HeraldConfig::default_path())?;
    let response = IpcClient::send(&config.daemon.socket_path, &IpcRequest::Shutdown).await?;
    println!("Daemon response: {:?}", response);
    Ok(())
}
