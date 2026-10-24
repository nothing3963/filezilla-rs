//! SFTP adapter built on `russh` + `russh-sftp` (pure Rust, no libssh2).

use crate::error::{AppError, AppResult};
use crate::models::{Protocol, RemoteEntry};
use crate::remote::{pump, ProgressCallback, RemoteFs, ResolvedConfig, TransferControl};
use crate::remote_path::{join, mode_to_string, normalize};
use crate::store::Store;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use russh::keys::{decode_secret_key, HashAlg, PrivateKeyWithHashAlg};
use russh::{
    client::{self, Handle},
    keys::PublicKeyOrCertificate,
    ChannelId, Disconnect,
};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

#[allow(dead_code)]
pub struct SftpRemote {
    session: Handle<ClientHandler>,
    sftp: SftpSession,
    home: String,
    banner: Option<String>,
}

struct HandlerState {
    host: String,
    port: u16,
    store: Arc<Store>,
    mismatch: Arc<Mutex<Option<String>>>,
    banner: Arc<Mutex<Option<String>>>,
    trusted_new: Arc<AtomicBool>,
}

pub struct ClientHandler {
    state: HandlerState,
}

#[derive(Debug)]
pub struct HandlerError(String);

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HandlerError {}

impl From<russh::Error> for HandlerError {
    fn from(value: russh::Error) -> Self {
        Self(value.to_string())
    }
}

impl client::Handler for ClientHandler {
    type Error = HandlerError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let key_type = key.algorithm().to_string();
        let known = self
            .state
            .store
            .known_host(&self.state.host, self.state.port, &key_type);
        match known {
            Some(expected) if expected == fingerprint => Ok(true),
            Some(expected) => {
                *self.state.mismatch.lock().unwrap() = Some(format!(
                    "Host key for {}:{} changed (expected {expected}, got {fingerprint}). \
                     Remove the saved key to trust the new one.",
                    self.state.host, self.state.port
                ));
                Ok(false)
            }
            None => {
                let accepted =
                    crate::hostkey::ask(&self.state.host, self.state.port, &key_type, &fingerprint)
                        .await;
                if accepted {
                    self.state.store.trust_known_host(
                        &self.state.host,
                        self.state.port,
                        &key_type,
                        &fingerprint,
                    );
                    self.state.trusted_new.store(true, Ordering::Relaxed);
                    Ok(true)
                } else {
                    *self.state.mismatch.lock().unwrap() = Some(format!(
                        "Host key for {}:{} was not trusted ({fingerprint}).",
                        self.state.host, self.state.port
                    ));
                    Ok(false)
                }
            }
        }
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        *self.state.banner.lock().unwrap() = Some(banner.trim().to_string());
        Ok(())
    }
}

impl SftpRemote {
    pub async fn connect(resolved: &ResolvedConfig) -> AppResult<Box<dyn RemoteFs>> {
        let cfg = &resolved.config;
        if cfg.username.trim().is_empty() {
            return Err(AppError::invalid("SFTP requires a username"));
        }
        let port = cfg.effective_port();
        let timeout = Duration::from_secs(cfg.connect_timeout_secs.max(3));
        let mismatch = Arc::new(Mutex::new(None));
        let banner = Arc::new(Mutex::new(None));
        let trusted_new = Arc::new(AtomicBool::new(false));
        let handler = ClientHandler {
            state: HandlerState {
                host: cfg.host.clone(),
                port,
                store: resolved.store.clone(),
                mismatch: mismatch.clone(),
                banner: banner.clone(),
                trusted_new: trusted_new.clone(),
            },
        };
        let mut config = russh::client::Config::default();
        config.inactivity_timeout = Some(Duration::from_secs(if cfg.keepalive_secs > 0 {
            cfg.keepalive_secs.max(15) * 3
        } else {
            300
        }));
        config.nodelay = true;

        let proxy_stream = if cfg.uses_proxy() {
            Some(
                crate::proxy::connect_tcp(
                    cfg,
                    resolved.proxy_password.as_deref(),
                    &cfg.host,
                    port,
                    timeout,
                )
                .await?,
            )
        } else {
            None
        };

        let mut session = tokio::time::timeout(timeout, async {
            match proxy_stream {
                Some(stream) => client::connect_stream(Arc::new(config), stream, handler).await,
                None => client::connect(Arc::new(config), (cfg.host.as_str(), port), handler).await,
            }
        })
        .await
        .map_err(|_| {
            AppError::new(
                "TIMEOUT",
                format!(
                    "Timed out connecting to {}:{} after {}s",
                    cfg.host,
                    port,
                    timeout.as_secs()
                ),
            )
        })?
        .map_err(|e| {
            if let Some(message) = mismatch.lock().unwrap().clone() {
                AppError::new("HOST_KEY", message)
            } else {
                AppError::new("NETWORK", format!("SSH connection failed: {e}"))
            }
        })?;

        let user = cfg.username.trim().to_string();
        let auth_result = match cfg.auth_method {
            crate::models::AuthMethod::Password => {
                let password = resolved.password.clone().unwrap_or_default();
                session
                    .authenticate_password(user.clone(), password)
                    .await
                    .map_err(|e| AppError::new("AUTH", format!("SSH authentication error: {e}")))?
            }
            crate::models::AuthMethod::Key => {
                let path = cfg
                    .private_key_path
                    .as_deref()
                    .ok_or_else(|| AppError::invalid("Private key file is required"))?;
                let pem = tokio::fs::read(path).await.map_err(|e| {
                    AppError::new("IO", format!("Cannot read private key {path}: {e}"))
                })?;
                let pem = String::from_utf8(pem)
                    .map_err(|_| AppError::invalid("Private key is not valid UTF-8"))?;
                let private = decode_secret_key(&pem, resolved.key_passphrase.as_deref())
                    .map_err(|e| AppError::new("AUTH", format!("Invalid private key: {e}")))?;
                let hash_alg = session
                    .best_supported_rsa_hash()
                    .await
                    .ok()
                    .flatten()
                    .flatten();
                let key = PrivateKeyWithHashAlg::new(Arc::new(private), hash_alg);
                session
                    .authenticate_publickey(user.clone(), key)
                    .await
                    .map_err(|e| {
                        AppError::new("AUTH", format!("SSH key authentication error: {e}"))
                    })?
            }
        };
        if !auth_result.success() {
            return Err(AppError::new(
                "AUTH",
                "Authentication failed. Check the username, password or key.",
            ));
        }

        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| AppError::new("NETWORK", format!("Cannot open SSH channel: {e}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| AppError::new("PROTOCOL", format!("Cannot start SFTP subsystem: {e}")))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| AppError::new("PROTOCOL", format!("SFTP handshake failed: {e}")))?;
        let home = sftp
            .canonicalize(".")
            .await
            .unwrap_or_else(|_| "/".to_string());

        let banner_text = banner.lock().unwrap().clone();
        Ok(Box::new(Self {
            session,
            sftp,
            home,
            banner: banner_text,
        }))
    }

    fn map_sftp(&self, error: impl std::fmt::Display) -> AppError {
        let text = error.to_string();
        let upper = text.to_ascii_uppercase();
        let code = if upper.contains("NO_SUCH_FILE") || upper.contains("NOT FOUND") {
            "NOT_FOUND"
        } else if upper.contains("PERMISSION_DENIED") || upper.contains("PERMISSION") {
            "PERMISSION"
        } else {
            "REMOTE"
        };
        AppError::new(code, text)
    }
}

fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
}

#[async_trait]
impl RemoteFs for SftpRemote {
    fn protocol(&self) -> Protocol {
        Protocol::Sftp
    }

    fn banner(&self) -> Option<String> {
        self.banner.clone()
    }

    fn features(&self) -> Vec<String> {
        vec!["SFTP v3".to_string()]
    }

    async fn list(&mut self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let path = normalize(path);
        let mut reader = self
            .sftp
            .read_dir(&path)
            .await
            .map_err(|e| self.map_sftp(e))?;
        let mut entries = Vec::new();
        while let Some(entry) = reader.next() {
            let meta = entry.metadata();
            let is_dir = meta.is_dir();
            let is_symlink = meta.is_symlink();
            let mode = meta.permissions;
            entries.push(RemoteEntry {
                name: entry.file_name(),
                path: join(&path, &entry.file_name()),
                is_dir,
                is_symlink,
                size: if is_dir { 0 } else { meta.size.unwrap_or(0) },
                modified: meta.modified().ok().and_then(system_time_to_rfc3339),
                permissions: mode.map(|m| mode_to_string(m, is_dir)),
                mode,
                owner: meta.user.clone(),
                group: meta.group.clone(),
            });
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    async fn stat(&mut self, path: &str) -> AppResult<RemoteEntry> {
        let path = normalize(path);
        let meta = self
            .sftp
            .metadata(&path)
            .await
            .map_err(|e| self.map_sftp(e))?;
        let is_dir = meta.is_dir();
        let name = if path == "/" {
            "/".to_string()
        } else {
            crate::remote_path::file_name(&path)
        };
        Ok(RemoteEntry {
            name,
            path,
            is_dir,
            is_symlink: meta.is_symlink(),
            size: meta.size.unwrap_or(0),
            modified: meta.modified().ok().and_then(system_time_to_rfc3339),
            permissions: meta.permissions.map(|m| mode_to_string(m, is_dir)),
            mode: meta.permissions,
            owner: meta.user.clone(),
            group: meta.group.clone(),
        })
    }

    async fn mkdir(&mut self, path: &str) -> AppResult<()> {
        self.sftp
            .create_dir(path)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn remove_file(&mut self, path: &str) -> AppResult<()> {
        self.sftp
            .remove_file(path)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn remove_dir(&mut self, path: &str) -> AppResult<()> {
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn rename(&mut self, from: &str, to: &str) -> AppResult<()> {
        self.sftp
            .rename(from, to)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn chmod(&mut self, path: &str, mode: u32) -> AppResult<()> {
        let mut attrs = self
            .sftp
            .metadata(path)
            .await
            .map_err(|e| self.map_sftp(e))?;
        attrs.permissions = Some(mode);
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn set_mtime(&mut self, path: &str, timestamp: i64) -> AppResult<()> {
        let mut attrs = self
            .sftp
            .metadata(path)
            .await
            .map_err(|e| self.map_sftp(e))?;
        attrs.mtime = Some(timestamp.max(0) as u32);
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn realpath(&mut self, path: &str) -> AppResult<String> {
        if path.is_empty() || path == "." {
            return self
                .sftp
                .canonicalize(".")
                .await
                .map_err(|e| self.map_sftp(e));
        }
        self.sftp
            .canonicalize(path)
            .await
            .map_err(|e| self.map_sftp(e))
    }

    async fn keepalive(&mut self) -> AppResult<()> {
        self.sftp
            .metadata(".")
            .await
            .map(|_| ())
            .map_err(|e| self.map_sftp(e))
    }

    async fn close(&mut self) -> AppResult<()> {
        let _ = self.sftp.close().await;
        let _ = self
            .session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
        Ok(())
    }

    async fn download(
        &mut self,
        remote: &str,
        local: &Path,
        resume_from: u64,
        ctl: &TransferControl,
        progress: ProgressCallback,
    ) -> AppResult<u64> {
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let mut local_file = if resume_from > 0 {
            let mut f = tokio::fs::OpenOptions::new()
                .append(true)
                .open(local)
                .await
                .map_err(|e| {
                    AppError::new("LOCAL_IO", format!("Cannot open {}: {e}", local.display()))
                })?;
            f.seek(std::io::SeekFrom::End(0)).await.ok();
            f
        } else {
            File::create(local).await.map_err(|e| {
                AppError::new(
                    "LOCAL_IO",
                    format!("Cannot create {}: {e}", local.display()),
                )
            })?
        };
        let mut remote_file = self.sftp.open(remote).await.map_err(|e| self.map_sftp(e))?;
        if resume_from > 0 {
            remote_file
                .seek(std::io::SeekFrom::Start(resume_from))
                .await
                .map_err(|e| AppError::new("TRANSFER", format!("Seek failed: {e}")))?;
        }
        let total = pump(
            &mut remote_file,
            &mut local_file,
            resume_from,
            ctl,
            &progress,
        )
        .await?;
        local_file.flush().await.ok();
        let _ = remote_file.close().await;
        Ok(total)
    }

    async fn upload(
        &mut self,
        local: &Path,
        remote: &str,
        resume_from: u64,
        ctl: &TransferControl,
        progress: ProgressCallback,
    ) -> AppResult<u64> {
        let mut local_file = File::open(local).await.map_err(|e| {
            AppError::new("LOCAL_IO", format!("Cannot read {}: {e}", local.display()))
        })?;
        if resume_from > 0 {
            local_file
                .seek(std::io::SeekFrom::Start(resume_from))
                .await?;
        }
        let flags = if resume_from > 0 {
            OpenFlags::CREATE | OpenFlags::WRITE
        } else {
            OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE
        };
        let mut remote_file = self
            .sftp
            .open_with_flags(remote, flags)
            .await
            .map_err(|e| self.map_sftp(e))?;
        if resume_from > 0 {
            remote_file
                .seek(std::io::SeekFrom::Start(resume_from))
                .await
                .map_err(|e| AppError::new("TRANSFER", format!("Seek failed: {e}")))?;
        }
        let total = pump(
            &mut local_file,
            &mut remote_file,
            resume_from,
            ctl,
            &progress,
        )
        .await?;
        remote_file
            .flush()
            .await
            .map_err(|e| AppError::new("TRANSFER", format!("Flush failed: {e}")))?;
        let _ = remote_file.sync_all().await;
        let _ = remote_file.close().await;
        Ok(total)
    }
}

#[allow(dead_code)]
fn unused(_: ChannelId) {}
