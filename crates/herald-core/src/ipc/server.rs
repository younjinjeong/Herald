use std::path::{Path, PathBuf};
use tokio::net::{TcpListener, UnixListener};
use tracing::{error, info, warn};

use crate::config::DaemonConfig;
use crate::error::{HeraldError, Result};
use crate::ipc::protocol::{read_message, write_message, IpcRequest, IpcResponse};

/// IPC server supporting Unix domain socket and/or TCP
pub struct IpcServer {
    socket_path: Option<PathBuf>,
    unix_listener: Option<UnixListener>,
    tcp_listener: Option<TcpListener>,
}

impl IpcServer {
    /// Bind based on daemon config transport mode
    pub async fn bind_from_config(config: &DaemonConfig) -> Result<Self> {
        let mut server = Self {
            socket_path: None,
            unix_listener: None,
            tcp_listener: None,
        };

        match config.transport.as_str() {
            "unix" => {
                server.bind_unix(&config.socket_path).await?;
            }
            "tcp" => {
                server.bind_tcp(&config.listen_addr).await?;
            }
            "both" => {
                server.bind_unix(&config.socket_path).await?;
                server.bind_tcp(&config.listen_addr).await?;
            }
            other => {
                return Err(HeraldError::Config(format!(
                    "Unknown transport: '{}'. Use 'unix', 'tcp', or 'both'",
                    other
                )));
            }
        }

        Ok(server)
    }

    /// Bind to Unix domain socket (backward compatible)
    pub async fn bind(socket_path: &Path) -> Result<Self> {
        let mut server = Self {
            socket_path: None,
            unix_listener: None,
            tcp_listener: None,
        };
        server.bind_unix(socket_path).await?;
        Ok(server)
    }

    async fn bind_unix(&mut self, socket_path: &Path) -> Result<()> {
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }

        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        info!("IPC server listening on Unix socket: {}", socket_path.display());
        self.socket_path = Some(socket_path.to_path_buf());
        self.unix_listener = Some(listener);
        Ok(())
    }

    async fn bind_tcp(&mut self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("IPC server listening on TCP: {}", addr);
        self.tcp_listener = Some(listener);
        Ok(())
    }

    /// Run the accept loop across all bound listeners
    pub async fn run<F, Fut>(&mut self, handler: F) -> Result<()>
    where
        F: Fn(IpcRequest) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = IpcResponse> + Send,
    {
        let unix = self.unix_listener.take();
        let tcp = self.tcp_listener.take();

        match (unix, tcp) {
            (Some(unix_listener), Some(tcp_listener)) => {
                // Both listeners — use select!
                loop {
                    let handler = handler.clone();
                    tokio::select! {
                        result = unix_listener.accept() => {
                            if let Ok((mut stream, _)) = result {
                                // Verify peer credentials for Unix connections
                                #[cfg(unix)]
                                {
                                    use crate::security::peercred;
                                    match peercred::verify_peer(&stream) {
                                        Ok(creds) => {
                                            info!(pid = creds.pid, uid = creds.uid, "Accepted Unix IPC connection");
                                        }
                                        Err(e) => {
                                            warn!("Rejected Unix IPC connection: {}", e);
                                            continue;
                                        }
                                    }
                                }
                                tokio::spawn(async move {
                                    handle_connection(&mut stream, handler).await;
                                });
                            }
                        }
                        result = tcp_listener.accept() => {
                            if let Ok((mut stream, addr)) = result {
                                info!("Accepted TCP IPC connection from {}", addr);
                                tokio::spawn(async move {
                                    handle_connection(&mut stream, handler).await;
                                });
                            }
                        }
                    }
                }
            }
            (Some(unix_listener), None) => {
                // Unix only
                loop {
                    let (mut stream, _) = unix_listener.accept().await?;
                    #[cfg(unix)]
                    {
                        use crate::security::peercred;
                        match peercred::verify_peer(&stream) {
                            Ok(creds) => {
                                info!(pid = creds.pid, uid = creds.uid, "Accepted Unix IPC connection");
                            }
                            Err(e) => {
                                warn!("Rejected Unix IPC connection: {}", e);
                                continue;
                            }
                        }
                    }
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        handle_connection(&mut stream, handler).await;
                    });
                }
            }
            (None, Some(tcp_listener)) => {
                // TCP only
                loop {
                    let (mut stream, addr) = tcp_listener.accept().await?;
                    info!("Accepted TCP IPC connection from {}", addr);
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        handle_connection(&mut stream, handler).await;
                    });
                }
            }
            (None, None) => {
                Err(HeraldError::Ipc("No listeners bound".to_string()))
            }
        }
    }

    pub fn socket_path(&self) -> Option<&Path> {
        self.socket_path.as_deref()
    }
}

/// Handle a single connection (works with any AsyncRead + AsyncWrite)
async fn handle_connection<S, F, Fut>(stream: &mut S, handler: F)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    F: Fn(IpcRequest) -> Fut,
    Fut: std::future::Future<Output = IpcResponse>,
{
    match read_message::<_, IpcRequest>(stream).await {
        Ok(request) => {
            let response = handler(request).await;
            if let Err(e) = write_message(stream, &response).await {
                error!("Failed to send IPC response: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to read IPC request: {}", e);
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if let Some(ref path) = self.socket_path {
            let _ = std::fs::remove_file(path);
        }
    }
}
