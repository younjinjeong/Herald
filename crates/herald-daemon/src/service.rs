use anyhow::Result;
use herald_core::config::HeraldConfig;
use herald_core::ipc::protocol::{IpcRequest, IpcResponse};
use herald_core::ipc::server::IpcServer;
use herald_core::session::registry::SessionRegistry;
use herald_core::session::token::generate_token;
use herald_core::types::{SessionId, SessionInfo, SessionState, SessionToken};
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{error, info};

pub async fn run(config: HeraldConfig) -> Result<()> {
    let registry = SessionRegistry::new();
    let start_time = chrono::Utc::now();
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // Start IPC server
    let mut ipc_server = IpcServer::bind(&config.daemon.socket_path).await?;

    let registry_clone = registry.clone();
    let ipc_handle = tokio::spawn(async move {
        let registry = registry_clone;
        let start = start_time;
        if let Err(e) = ipc_server
            .run(move |request| {
                let registry = registry.clone();
                async move { handle_request(request, &registry, start).await }
            })
            .await
        {
            error!("IPC server error: {}", e);
        }
    });

    // Start signal handler
    let signal_handle = tokio::spawn(async move {
        crate::signal::wait_for_shutdown().await;
        let _ = shutdown_tx.send(true);
    });

    // Wait for shutdown signal
    shutdown_rx.changed().await?;
    info!("Shutting down Herald daemon...");

    ipc_handle.abort();
    signal_handle.abort();

    Ok(())
}

async fn handle_request(
    request: IpcRequest,
    registry: &SessionRegistry,
    start_time: chrono::DateTime<chrono::Utc>,
) -> IpcResponse {
    match request {
        IpcRequest::Register {
            session_id,
            pid,
            cwd,
        } => {
            let token = generate_token();
            let info = SessionInfo {
                id: SessionId(session_id.clone()),
                token: token.clone(),
                pid,
                cwd: cwd.into(),
                state: SessionState::Active,
                started_at: chrono::Utc::now(),
                last_activity: chrono::Utc::now(),
            };
            match registry.register(info).await {
                Ok(()) => {
                    info!("Session registered: {} (PID {})", session_id, pid);
                    IpcResponse::Registered { token: token.0 }
                }
                Err(e) => IpcResponse::Error {
                    code: 500,
                    message: e.to_string(),
                },
            }
        }
        IpcRequest::Unregister { session_id, token } => {
            if !registry.validate_token(&session_id, &token).await {
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            match registry.unregister(&session_id).await {
                Ok(()) => {
                    info!("Session unregistered: {}", session_id);
                    IpcResponse::Ok {
                        message: Some("Session unregistered".to_string()),
                    }
                }
                Err(e) => IpcResponse::Error {
                    code: 404,
                    message: e.to_string(),
                },
            }
        }
        IpcRequest::Output {
            session_id, token, ..
        } => {
            if !registry.validate_token(&session_id, &token).await {
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            registry.update_activity(&session_id).await;
            // TODO: Forward to Telegram via message queue
            IpcResponse::Ok { message: None }
        }
        IpcRequest::Notification {
            session_id, token, ..
        } => {
            if !registry.validate_token(&session_id, &token).await {
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            // TODO: Forward notification to Telegram
            IpcResponse::Ok { message: None }
        }
        IpcRequest::SessionStopped {
            session_id, token, ..
        } => {
            if !registry.validate_token(&session_id, &token).await {
                return IpcResponse::Error {
                    code: 401,
                    message: "Invalid session token".to_string(),
                };
            }
            let _ = registry.unregister(&session_id).await;
            IpcResponse::Ok { message: None }
        }
        IpcRequest::Input { .. } => {
            // TODO: Route to headless or PTY
            IpcResponse::Ok {
                message: Some("Input received".to_string()),
            }
        }
        IpcRequest::Health => {
            let uptime = (chrono::Utc::now() - start_time).num_seconds() as u64;
            let session_count = registry.count().await;
            IpcResponse::HealthStatus {
                uptime_secs: uptime,
                session_count,
                telegram_connected: false, // TODO: Check actual Telegram connection
            }
        }
        IpcRequest::ListSessions => {
            let sessions = registry.list().await;
            IpcResponse::SessionList { sessions }
        }
        IpcRequest::Shutdown => {
            info!("Shutdown requested via IPC");
            IpcResponse::Ok {
                message: Some("Shutting down".to_string()),
            }
        }
    }
}
