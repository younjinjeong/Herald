use anyhow::Result;
use tracing::info;

mod headless;
mod pty;
mod queue;
mod service;
mod signal;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("heraldd=info".parse()?),
        )
        .init();

    info!("Herald daemon starting...");

    let config = herald_core::config::HeraldConfig::load(
        &herald_core::config::HeraldConfig::default_path(),
    )?;

    // Notify systemd that we are ready (Linux with systemd only)
    #[cfg(feature = "systemd")]
    {
        let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
    }

    info!("Herald daemon ready");

    // Run the main service
    service::run(config).await?;

    Ok(())
}
