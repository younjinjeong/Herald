use anyhow::Result;
use herald_core::config::HeraldConfig;
use herald_core::ipc::client::IpcClient;
use herald_core::ipc::protocol::IpcRequest;
use std::io::Read;

pub async fn run(session: &str, message: &str) -> Result<()> {
    let config = HeraldConfig::load(&HeraldConfig::default_path())?;
    let request = IpcRequest::Input {
        session_id: session.to_string(),
        prompt: message.to_string(),
    };
    let response = IpcClient::send(&config.daemon.socket_path, &request).await?;
    println!("{:?}", response);
    Ok(())
}

pub async fn ipc_send() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let request: IpcRequest = serde_json::from_str(&input)?;
    let config = HeraldConfig::load(&HeraldConfig::default_path())?;
    let response = IpcClient::send(&config.daemon.socket_path, &request).await?;

    let output = serde_json::to_string(&response)?;
    println!("{}", output);
    Ok(())
}
