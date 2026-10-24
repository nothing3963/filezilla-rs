//! Protocol independent remote file system contract.

use crate::error::{AppError, AppResult};
use crate::models::{Protocol, RemoteEntry, SiteConfig};
use async_trait::async_trait;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

/// Fully resolved connection parameters (site config + secrets).
#[derive(Clone)]
pub struct ResolvedConfig {
    #[allow(dead_code)]
    pub site_id: Option<String>,
    pub name: String,
    pub config: SiteConfig,
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
    pub proxy_password: Option<String>,
    /// Shared store used for SSH known-host (TOFU) bookkeeping.
    pub store: Arc<crate::store::Store>,
}

pub type ProgressCallback = Arc<dyn Fn(u64) + Send + Sync>;

/// Cooperative pause / cancel / rate-limit state shared with a running transfer.
pub struct TransferControl {
    pub cancel: CancellationToken,
    pub paused: Arc<AtomicBool>,
    pub speed_limit_bps: Arc<AtomicU64>,
    started: Instant,
}

impl TransferControl {
    pub fn new(
        cancel: CancellationToken,
        paused: Arc<AtomicBool>,
        speed_limit_bps: Arc<AtomicU64>,
    ) -> Self {
        Self {
            cancel,
            paused,
            speed_limit_bps,
            started: Instant::now(),
        }
    }

    /// Returns once the transfer may continue; aborts when cancelled.
    pub async fn checkpoint(&self) -> AppResult<()> {
        if self.cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        while self.paused.load(Ordering::Relaxed) {
            tokio::select! {
                _ = self.cancel.cancelled() => return Err(AppError::cancelled()),
                _ = tokio::time::sleep(Duration::from_millis(80)) => {}
            }
            if self.cancel.is_cancelled() {
                return Err(AppError::cancelled());
            }
        }
        Ok(())
    }

    /// Rough token bucket: keeps the average rate near the configured limit.
    pub async fn throttle(&self, total: u64) {
        let limit = self.speed_limit_bps.load(Ordering::Relaxed);
        if limit == 0 || total == 0 {
            return;
        }
        let expected = Duration::from_secs_f64(total as f64 / limit as f64);
        let elapsed = self.started.elapsed();
        if expected > elapsed {
            tokio::select! {
                _ = self.cancel.cancelled() => {}
                _ = tokio::time::sleep(expected - elapsed) => {}
            }
        }
    }
}

#[async_trait]
pub trait RemoteFs: Send {
    #[allow(dead_code)]
    fn protocol(&self) -> Protocol;
    fn banner(&self) -> Option<String>;
    fn features(&self) -> Vec<String>;
    /// Encoding negotiated/detected for this connection (FTP only, `None`
    /// for SFTP).  Transfer connections reuse this to address legacy names.
    fn encoding(&self) -> Option<String> {
        None
    }

    async fn list(&mut self, path: &str) -> AppResult<Vec<RemoteEntry>>;
    async fn stat(&mut self, path: &str) -> AppResult<RemoteEntry>;
    async fn mkdir(&mut self, path: &str) -> AppResult<()>;
    async fn remove_file(&mut self, path: &str) -> AppResult<()>;
    async fn remove_dir(&mut self, path: &str) -> AppResult<()>;
    async fn rename(&mut self, from: &str, to: &str) -> AppResult<()>;
    async fn chmod(&mut self, path: &str, mode: u32) -> AppResult<()>;
    /// Set the remote modification time (FTP MFMT / SFTP setstat).  Servers
    /// that do not support it may return an error; transfers treat that as
    /// non-fatal.
    async fn set_mtime(&mut self, _path: &str, _timestamp: i64) -> AppResult<()> {
        Err(AppError::unsupported(
            "This protocol does not support setting modification times",
        ))
    }
    /// Canonicalize a path; implementations fall back to `normalize`.
    async fn realpath(&mut self, path: &str) -> AppResult<String>;
    async fn keepalive(&mut self) -> AppResult<()>;
    async fn close(&mut self) -> AppResult<()>;

    async fn download(
        &mut self,
        remote: &str,
        local: &Path,
        resume_from: u64,
        ctl: &TransferControl,
        progress: ProgressCallback,
    ) -> AppResult<u64>;

    async fn upload(
        &mut self,
        local: &Path,
        remote: &str,
        resume_from: u64,
        ctl: &TransferControl,
        progress: ProgressCallback,
    ) -> AppResult<u64>;
}

pub async fn connect(resolved: &ResolvedConfig) -> AppResult<Box<dyn RemoteFs>> {
    match resolved.config.protocol {
        Protocol::Ftp | Protocol::Ftps | Protocol::FtpsImplicit => {
            crate::ftp::FtpRemote::connect(resolved).await
        }
        Protocol::Sftp => crate::sftp::SftpRemote::connect(resolved).await,
    }
}

/// Copy a stream while keeping the UI informed and honouring pause/cancel/rate limits.
pub async fn pump<R, W>(
    reader: &mut R,
    writer: &mut W,
    offset: u64,
    ctl: &TransferControl,
    progress: &ProgressCallback,
) -> AppResult<u64>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    let mut buffer = vec![0u8; 128 * 1024];
    let mut total = offset;
    loop {
        ctl.checkpoint().await?;
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|e| AppError::new("TRANSFER", format!("Read failed: {e}")))?;
        if read == 0 {
            break;
        }
        writer
            .write_all(&buffer[..read])
            .await
            .map_err(|e| AppError::new("TRANSFER", format!("Write failed: {e}")))?;
        total += read as u64;
        progress(total);
        ctl.throttle(total - offset).await;
    }
    writer.flush().await.ok();
    Ok(total)
}
