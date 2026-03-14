use std::path::{Path, PathBuf};
use tokio::net::UnixListener;
use tracing::{error, info, warn};

use crate::error::{HeraldError, Result};
use crate::ipc::protocol::{read_message, write_message, IpcRequest, IpcResponse};
use crate::security::peercred;

/// IPC server listening on a Unix domain socket
pub struct IpcServer {
    socket_path: PathBuf,
    listener: Option<UnixListener>,
}

impl IpcServer {
    /// Bind to the given socket path, creating parent directories as needed
    pub async fn bind(socket_path: &Path) -> Result<Self> {
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }

        // Remove existing socket file
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        info!("IPC server listening on {}", socket_path.display());

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            listener: Some(listener),
        })
    }

    /// Run the accept loop, dispatching requests to the handler
    pub async fn run<F, Fut>(&mut self, handler: F) -> Result<()>
    where
        F: Fn(IpcRequest) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = IpcResponse> + Send,
    {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| HeraldError::Ipc("Server already running".to_string()))?;

        loop {
            let (mut stream, _addr) = listener.accept().await?;

            // Verify peer credentials (same UID)
            match peercred::verify_peer(&stream) {
                Ok(creds) => {
                    info!(pid = creds.pid, uid = creds.uid, "Accepted IPC connection");
                }
                Err(e) => {
                    warn!("Rejected IPC connection: {}", e);
                    continue;
                }
            }

            let handler = handler.clone();
            tokio::spawn(async move {
                match read_message::<_, IpcRequest>(&mut stream).await {
                    Ok(request) => {
                        let response = handler(request).await;
                        if let Err(e) = write_message(&mut stream, &response).await {
                            error!("Failed to send IPC response: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to read IPC request: {}", e);
                    }
                }
            });
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
