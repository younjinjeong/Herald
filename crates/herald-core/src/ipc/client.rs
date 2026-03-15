use std::path::{Path, PathBuf};
use tokio::net::{TcpStream, UnixStream};

use crate::error::Result;
use crate::ipc::protocol::{read_message, write_message, IpcRequest, IpcResponse};

/// Transport type for IPC connections
#[derive(Debug, Clone)]
pub enum IpcTransport {
    /// Unix domain socket (local only)
    Unix(PathBuf),
    /// TCP socket (local or remote)
    Tcp(String), // "host:port"
}

impl IpcTransport {
    /// Create from config: if HERALD_DAEMON_ADDR env is set, use TCP; otherwise Unix
    pub fn from_config(socket_path: &Path, listen_addr: &str, transport: &str) -> Self {
        // Environment override for remote connections
        if let Ok(addr) = std::env::var("HERALD_DAEMON_ADDR") {
            return Self::Tcp(addr);
        }

        match transport {
            "tcp" => Self::Tcp(listen_addr.to_string()),
            _ => Self::Unix(socket_path.to_path_buf()),
        }
    }
}

pub struct IpcClient;

impl IpcClient {
    /// Send a request via the specified transport
    pub async fn send_via(
        transport: &IpcTransport,
        request: &IpcRequest,
    ) -> Result<IpcResponse> {
        match transport {
            IpcTransport::Unix(path) => Self::send_unix(path, request).await,
            IpcTransport::Tcp(addr) => Self::send_tcp(addr, request).await,
        }
    }

    /// Send via Unix domain socket (backward compatible)
    pub async fn send(socket_path: &Path, request: &IpcRequest) -> Result<IpcResponse> {
        Self::send_unix(socket_path, request).await
    }

    async fn send_unix(socket_path: &Path, request: &IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(socket_path).await?;
        write_message(&mut stream, request).await?;
        let response: IpcResponse = read_message(&mut stream).await?;
        Ok(response)
    }

    async fn send_tcp(addr: &str, request: &IpcRequest) -> Result<IpcResponse> {
        let mut stream = TcpStream::connect(addr).await?;
        write_message(&mut stream, request).await?;
        let response: IpcResponse = read_message(&mut stream).await?;
        Ok(response)
    }
}
