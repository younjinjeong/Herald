use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{HeraldError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeraldConfig {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub credentials: CredentialsConfig,
    #[serde(default)]
    pub output_filter: OutputFilterConfig,
    #[serde(default)]
    pub sessions: SessionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_output")]
    pub log_output: String,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default = "default_pid_file")]
    pub pid_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_network_mode")]
    pub mode: String,
    pub proxy_url: Option<String>,
    #[serde(default = "default_polling_timeout")]
    pub polling_timeout_seconds: u64,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_seconds: u64,
    #[serde(default = "default_backoff_initial")]
    pub backoff_initial_seconds: u64,
    #[serde(default = "default_backoff_max")]
    pub backoff_max_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
    #[serde(default = "default_otp_length")]
    pub otp_length: usize,
    #[serde(default = "default_otp_timeout")]
    pub otp_timeout_seconds: u64,
    #[serde(default = "default_otp_max_attempts")]
    pub otp_max_attempts: u32,
    #[serde(default = "default_otp_lockout")]
    pub otp_lockout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialsConfig {
    #[serde(default = "default_storage")]
    pub storage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFilterConfig {
    #[serde(default = "default_filter_mode")]
    pub mode: String,
    #[serde(default = "default_max_message_length")]
    pub max_message_length: usize,
    #[serde(default = "default_code_preview_lines")]
    pub code_preview_lines: usize,
    #[serde(default = "default_true")]
    pub mask_secrets: bool,
    #[serde(default = "default_secret_patterns")]
    pub secret_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_auto_cleanup_minutes")]
    pub auto_cleanup_minutes: u64,
}

fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("herald").join("herald.sock")
    } else {
        PathBuf::from("/tmp").join("herald").join("herald.sock")
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:7272".to_string()
}
fn default_transport() -> String {
    "unix".to_string()
}
fn default_log_level() -> String {
    "INFO".to_string()
}
fn default_log_output() -> String {
    if std::env::var("HERALD_CONTAINER").is_ok() {
        "stdout".to_string()
    } else {
        "file".to_string()
    }
}
fn default_auth_mode() -> String {
    "peercred".to_string()
}

fn default_pid_file() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("herald").join("herald.pid")
    } else {
        PathBuf::from("/tmp").join("herald").join("herald.pid")
    }
}

fn default_network_mode() -> String {
    "direct".to_string()
}
fn default_polling_timeout() -> u64 {
    30
}
fn default_connection_timeout() -> u64 {
    10
}
fn default_backoff_initial() -> u64 {
    1
}
fn default_backoff_max() -> u64 {
    300
}
fn default_otp_length() -> usize {
    6
}
fn default_otp_timeout() -> u64 {
    300
}
fn default_otp_max_attempts() -> u32 {
    3
}
fn default_otp_lockout() -> u64 {
    600
}
fn default_storage() -> String {
    "keyring".to_string()
}
fn default_filter_mode() -> String {
    "summary".to_string()
}
fn default_max_message_length() -> usize {
    4096
}
fn default_code_preview_lines() -> usize {
    5
}
fn default_true() -> bool {
    true
}

fn default_secret_patterns() -> Vec<String> {
    vec![
        r"(?i)(api[_\-]?key|token|secret|password|passwd)\s*[=:]\s*\S+".to_string(),
        r"(?i)bearer\s+\S+".to_string(),
        r"-----BEGIN .* PRIVATE KEY-----".to_string(),
    ]
}

fn default_max_concurrent() -> usize {
    10
}
fn default_auto_cleanup_minutes() -> u64 {
    30
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            listen_addr: default_listen_addr(),
            transport: default_transport(),
            log_level: default_log_level(),
            log_output: default_log_output(),
            auth_mode: default_auth_mode(),
            pid_file: default_pid_file(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: default_network_mode(),
            proxy_url: None,
            polling_timeout_seconds: default_polling_timeout(),
            connection_timeout_seconds: default_connection_timeout(),
            backoff_initial_seconds: default_backoff_initial(),
            backoff_max_seconds: default_backoff_max(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            allowed_chat_ids: vec![],
            otp_length: default_otp_length(),
            otp_timeout_seconds: default_otp_timeout(),
            otp_max_attempts: default_otp_max_attempts(),
            otp_lockout_seconds: default_otp_lockout(),
        }
    }
}

impl Default for CredentialsConfig {
    fn default() -> Self {
        Self {
            storage: default_storage(),
        }
    }
}

impl Default for OutputFilterConfig {
    fn default() -> Self {
        Self {
            mode: default_filter_mode(),
            max_message_length: default_max_message_length(),
            code_preview_lines: default_code_preview_lines(),
            mask_secrets: true,
            secret_patterns: default_secret_patterns(),
        }
    }
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            auto_cleanup_minutes: default_auto_cleanup_minutes(),
        }
    }
}

impl Default for HeraldConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            network: NetworkConfig::default(),
            auth: AuthConfig::default(),
            credentials: CredentialsConfig::default(),
            output_filter: OutputFilterConfig::default(),
            sessions: SessionsConfig::default(),
        }
    }
}

impl HeraldConfig {
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("herald")
            .join("config.toml")
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| HeraldError::Config(format!("Failed to read config: {}", e)))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| HeraldError::Config(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| HeraldError::Config(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get_bot_token(&self) -> Result<String> {
        // 1. Environment variable (highest priority)
        if let Ok(token) = std::env::var("HERALD_BOT_TOKEN") {
            return Ok(token);
        }

        // 2. Try keyring
        if self.credentials.storage == "keyring" || self.credentials.storage == "auto" {
            if let Ok(entry) = keyring::Entry::new("herald", "bot_token") {
                if let Ok(token) = entry.get_password() {
                    if !token.is_empty() {
                        return Ok(token);
                    }
                }
            }
        }

        // 3. Fallback: read from token file
        let token_path = Self::token_file_path();
        if token_path.exists() {
            let token = std::fs::read_to_string(&token_path)
                .map_err(|e| HeraldError::Config(format!("Failed to read token file: {}", e)))?;
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Ok(token);
            }
        }

        Err(HeraldError::Config(
            "Bot token not found. Run `herald setup` or set HERALD_BOT_TOKEN env var.".to_string(),
        ))
    }

    pub fn set_bot_token(token: &str) -> Result<()> {
        // Try keyring first
        let keyring_ok = if let Ok(entry) = keyring::Entry::new("herald", "bot_token") {
            // Test if keyring actually works by trying a set+get round-trip
            if entry.set_password(token).is_ok() {
                // Verify it persisted (catches mock backends)
                entry.get_password().map(|t| t == token).unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        if keyring_ok {
            tracing::info!("Bot token stored in system keyring");
            return Ok(());
        }

        // Fallback: store in file with restricted permissions
        tracing::warn!("System keyring not available, storing token in file");
        let token_path = Self::token_file_path();
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&token_path, token)?;

        // Set file permissions to owner-only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))?;
        }

        tracing::info!("Bot token stored in {}", token_path.display());
        Ok(())
    }

    fn token_file_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("herald")
            .join(".bot_token")
    }
}
