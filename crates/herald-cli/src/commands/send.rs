use anyhow::Result;
use herald_core::config::HeraldConfig;
use herald_core::ipc::client::{IpcClient, IpcTransport};
use herald_core::ipc::protocol::IpcRequest;
use std::io::Read;

fn get_transport(tcp: Option<&str>) -> Result<IpcTransport> {
    // CLI --tcp flag takes priority
    if let Some(addr) = tcp {
        return Ok(IpcTransport::Tcp(addr.to_string()));
    }
    // Then env var
    if let Ok(addr) = std::env::var("HERALD_DAEMON_ADDR") {
        return Ok(IpcTransport::Tcp(addr));
    }
    // Then config
    let config = HeraldConfig::load(&HeraldConfig::default_path())?;
    Ok(IpcTransport::from_config(
        &config.daemon.socket_path,
        &config.daemon.listen_addr,
        &config.daemon.transport,
    ))
}

pub async fn run(session: &str, message: &str, tcp: Option<&str>) -> Result<()> {
    let transport = get_transport(tcp)?;
    let request = IpcRequest::Input {
        session_id: session.to_string(),
        prompt: message.to_string(),
    };
    let response = IpcClient::send_via(&transport, &request).await?;
    println!("{:?}", response);
    Ok(())
}

pub async fn ipc_send(tcp: Option<&str>) -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let request: IpcRequest = serde_json::from_str(&input)?;
    let transport = get_transport(tcp)?;
    let response = IpcClient::send_via(&transport, &request).await?;

    let output = serde_json::to_string(&response)?;
    println!("{}", output);
    Ok(())
}
