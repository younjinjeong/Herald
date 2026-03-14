use tokio::signal::unix::{signal, SignalKind};
use tracing::info;

pub async fn wait_for_shutdown() {
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
    let mut sighup = signal(SignalKind::hangup()).expect("Failed to register SIGHUP handler");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM");
        }
        _ = sighup.recv() => {
            info!("Received SIGHUP - reloading config");
            // TODO: Implement config reload
        }
    }
}
