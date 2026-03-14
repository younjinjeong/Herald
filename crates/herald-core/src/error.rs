use thiserror::Error;

#[derive(Error, Debug)]
pub enum HeraldError {
    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Telegram error: {0}")]
    Telegram(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Security violation: {0}")]
    Security(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, HeraldError>;
