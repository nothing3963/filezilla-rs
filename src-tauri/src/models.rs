use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable, serializable error shape shared with the frontend.
///
/// `code` is meant for programmatic handling / i18n, `message` is a short
/// human readable summary and `detail` carries the underlying protocol error
/// when it adds useful context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.is_empty() {
            self.detail = Some(detail);
        }
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("UNSUPPORTED", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL", message)
    }

    pub fn cancelled() -> Self {
        Self::new("CANCELLED", "Transfer cancelled")
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        let code = match value.kind() {
            std::io::ErrorKind::NotFound => "NOT_FOUND",
            std::io::ErrorKind::PermissionDenied => "PERMISSION",
            _ => "IO",
        };
        Self::new(code, value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::new("INTERNAL", format!("JSON error: {value}"))
    }
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        Self::new("INTERNAL", value.to_string())
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    #[serde(rename = "ftp")]
    Ftp,
    /// FTP over explicit TLS (AUTH TLS on port 21 by default).
    #[serde(rename = "ftps")]
    Ftps,
    /// FTP over implicit TLS (port 990 by default).
    #[serde(rename = "ftpsi")]
    FtpsImplicit,
    #[serde(rename = "sftp")]
    Sftp,
}

impl Protocol {
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Ftp | Protocol::Ftps => 21,
            Protocol::FtpsImplicit => 990,
            Protocol::Sftp => 22,
        }
    }

    pub fn uses_tls(self) -> bool {
        matches!(self, Protocol::Ftps | Protocol::FtpsImplicit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    #[default]
    Password,
    Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TransferMode {
    #[default]
    Auto,
    Binary,
    Ascii,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    20
}

fn default_connections() -> u8 {
    2
}

fn default_encoding() -> String {
    "UTF-8".to_string()
}

/// Everything needed to open a connection except the secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteConfig {
    pub protocol: Protocol,
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub auth_method: AuthMethod,
    #[serde(default)]
    pub private_key_path: Option<String>,
    #[serde(default)]
    pub initial_remote_dir: Option<String>,
    #[serde(default)]
    pub local_dir: Option<String>,
    #[serde(default = "default_true")]
    pub passive_mode: bool,
    #[serde(default = "default_true")]
    pub verify_certificate: bool,
    #[serde(default = "default_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default)]
    pub keepalive_secs: u64,
    #[serde(default)]
    pub transfer_mode: TransferMode,
    #[serde(default = "default_encoding")]
    pub encoding: String,
    #[serde(default = "default_connections")]
    pub max_connections: u8,
    #[serde(default)]
    pub notes: Option<String>,
    /// "", "http" or "socks5"
    #[serde(default)]
    pub proxy_type: String,
    #[serde(default)]
    pub proxy_host: String,
    #[serde(default)]
    pub proxy_port: u16,
    #[serde(default)]
    pub proxy_username: String,
}

impl SiteConfig {
    pub fn effective_port(&self) -> u16 {
        if self.port == 0 {
            self.protocol.default_port()
        } else {
            self.port
        }
    }

    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.effective_port())
    }

    pub fn uses_proxy(&self) -> bool {
        !matches!(self.proxy_type.as_str(), "" | "none")
    }

    pub fn validate(&mut self) -> AppResult<()> {
        self.host = self.host.trim().to_string();
        if self.host.is_empty() {
            return Err(AppError::invalid("Host is required"));
        }
        if self.host.contains(char::is_whitespace) {
            return Err(AppError::invalid("Host must not contain whitespace"));
        }
        if self.port == 0 {
            self.port = self.protocol.default_port();
        }
        self.username = self.username.trim().to_string();
        self.proxy_type = self.proxy_type.trim().to_ascii_lowercase();
        if !matches!(self.proxy_type.as_str(), "" | "none" | "http" | "socks5") {
            return Err(AppError::invalid("Unsupported proxy type"));
        }
        if !self.proxy_type.is_empty() && self.proxy_type != "none" {
            if self.proxy_host.trim().is_empty() {
                return Err(AppError::invalid("Proxy host is required"));
            }
            if self.proxy_port == 0 {
                self.proxy_port = if self.proxy_type == "http" {
                    8080
                } else {
                    1080
                };
            }
        }
        if self.protocol == Protocol::Sftp && self.auth_method == AuthMethod::Key {
            if self
                .private_key_path
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(AppError::invalid("A private key file is required"));
            }
        }
        self.max_connections = self.max_connections.clamp(1, 10);
        self.connect_timeout_secs = self.connect_timeout_secs.clamp(3, 300);
        Ok(())
    }
}

/// Secrets supplied by the UI for a single connection attempt.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_passphrase: Option<String>,
    #[serde(default)]
    pub proxy_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub group_id: Option<String>,
    pub config: SiteConfig,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub group_id: Option<String>,
    pub config: SiteConfig,
    /// When omitted the previously stored password is kept.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_passphrase: Option<String>,
    #[serde(default)]
    pub proxy_password: Option<String>,
    #[serde(default)]
    pub clear_password: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitesSnapshot {
    pub sites: Vec<Site>,
    pub groups: Vec<SiteGroup>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    /// Connect using a stored site (configuration and secrets are loaded by id).
    #[serde(default)]
    pub site_id: Option<String>,
    /// Quick-connect / ad-hoc configuration used when `site_id` is absent.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub config: Option<SiteConfig>,
    #[serde(default)]
    pub credentials: Option<Credentials>,
    /// Persist the password for the stored site after a successful login.
    #[serde(default)]
    pub save_password: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub name: String,
    pub home_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    pub site_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<RemoteEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalKind {
    File,
    Directory,
    Drive,
    Place,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntry {
    pub name: String,
    pub path: String,
    pub kind: LocalKind,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_space: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalListing {
    /// `None` means the virtual "This PC" root.
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<LocalEntry>,
    pub places: Vec<LocalEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferDirection {
    Download,
    Upload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferStatus {
    Queued,
    Connecting,
    Active,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl TransferStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferTask {
    pub id: String,
    pub session_id: String,
    #[serde(default)]
    pub site_id: Option<String>,
    pub site_name: String,
    pub direction: TransferDirection,
    pub local_path: String,
    pub remote_path: String,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub resumed_from: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<i64>,
    pub status: TransferStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub speed_bps: u64,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    pub direction: TransferDirection,
    pub local_path: String,
    pub remote_path: String,
    #[serde(default)]
    pub size: Option<u64>,
    /// When set, the backend expands a directory into individual file tasks.
    #[serde(default)]
    pub is_dir: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRequest {
    pub session_id: String,
    pub items: Vec<TransferItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    Remove,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub active: u32,
    pub queued: u32,
    pub completed: u32,
    pub failed: u32,
    pub total_speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: String,
    pub name: String,
    /// "local" or "remote"
    pub side: String,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_density")]
    pub density: String,
    #[serde(default)]
    pub show_hidden_files: bool,
    #[serde(default = "default_true")]
    pub confirm_delete: bool,
    #[serde(default = "default_double_click")]
    pub double_click_action: String,
    #[serde(default = "default_connections")]
    pub concurrent_transfers: u8,
    #[serde(default)]
    pub download_limit_kbps: u32,
    #[serde(default)]
    pub upload_limit_kbps: u32,
    #[serde(default = "default_true")]
    pub resume_transfers: bool,
    #[serde(default = "default_true")]
    pub preserve_timestamps: bool,
    #[serde(default)]
    pub sync_browsing: bool,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default = "default_true")]
    pub notify_on_complete: bool,
    #[serde(default)]
    pub sound_on_complete: bool,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub default_local_dir: Option<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub show_log_panel: bool,
    #[serde(default)]
    pub filezilla_import_prompted: bool,
    #[serde(default)]
    pub animations: bool,
    #[serde(default = "default_true")]
    pub mica_effect: bool,
    #[serde(default = "default_true")]
    pub toolbar_labels: bool,
}

fn default_accent() -> String {
    "blue".into()
}

fn default_theme() -> String {
    "system".into()
}
fn default_language() -> String {
    // Empty means "not chosen yet": the frontend picks the best match for the
    // operating system language and only stores a code once the user selects one.
    String::new()
}
fn default_density() -> String {
    "comfortable".into()
}
fn default_double_click() -> String {
    "transfer".into()
}
fn default_log_level() -> String {
    "info".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            accent: default_accent(),
            language: default_language(),
            density: default_density(),
            show_hidden_files: false,
            confirm_delete: true,
            double_click_action: default_double_click(),
            concurrent_transfers: default_connections(),
            download_limit_kbps: 0,
            upload_limit_kbps: 0,
            resume_transfers: true,
            preserve_timestamps: true,
            sync_browsing: false,
            bookmarks: Vec::new(),
            notify_on_complete: true,
            sound_on_complete: false,
            minimize_to_tray: false,
            default_local_dir: None,
            log_level: default_log_level(),
            show_log_panel: false,
            filezilla_import_prompted: false,
            animations: false,
            mica_effect: true,
            toolbar_labels: true,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: i64,
    pub level: String,
    pub scope: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
    pub config_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub host: String,
    pub key_type: String,
    pub fingerprint: String,
    pub added_at: i64,
}
