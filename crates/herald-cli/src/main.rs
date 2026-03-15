use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod wizard;

#[derive(Parser)]
#[command(name = "herald", about = "Herald - Claude Code Telegram Remote Control")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard
    Setup,
    /// Start the Herald daemon
    Start,
    /// Stop the Herald daemon
    Stop,
    /// Show daemon and session status
    Status,
    /// Send a prompt to a session
    Send {
        /// Session ID
        session: String,
        /// Prompt message
        message: String,
        /// Connect via TCP instead of Unix socket (host:port)
        #[arg(long)]
        tcp: Option<String>,
    },
    /// Internal: send raw IPC message from stdin (used by hook scripts)
    #[command(hide = true)]
    IpcSend {
        /// Connect via TCP instead of Unix socket (host:port)
        #[arg(long)]
        tcp: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("herald=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Setup => commands::setup::run().await?,
        Commands::Start => commands::start::run().await?,
        Commands::Stop => commands::stop::run().await?,
        Commands::Status => commands::status::run().await?,
        Commands::Send {
            session,
            message,
            tcp,
        } => commands::send::run(&session, &message, tcp.as_deref()).await?,
        Commands::IpcSend { tcp } => commands::send::ipc_send(tcp.as_deref()).await?,
    }

    Ok(())
}
