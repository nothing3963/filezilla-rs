//! Remote file editing: download a file to a private temp directory, watch
//! local saves, and upload the new content back to the same remote path.
//!
//! A polling watcher is used instead of an OS filesystem watcher so it works
//! identically for every editor and does not need per-platform dependencies.
//! Saves are debounced by requiring a stable size/mtime for two ticks.

use crate::error::{AppError, AppResult};
use crate::remote::{self, TransferControl};
use crate::session::SessionManager;
use crate::store::Store;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditStart {
    pub local_path: String,
    pub remote_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EditUploaded {
    local_path: String,
    remote_path: String,
    at: i64,
}

struct EditSession {
    cancel: CancellationToken,
}

pub struct EditManager {
    app: AppHandle,
    sessions: Arc<SessionManager>,
    store: Arc<Store>,
    active: Mutex<HashMap<String, EditSession>>,
    upload_lock: AsyncMutex<()>,
}

impl EditManager {
    pub fn new(app: AppHandle, sessions: Arc<SessionManager>, store: Arc<Store>) -> Arc<Self> {
        Arc::new(Self {
            app,
            sessions,
            store,
            active: Mutex::new(HashMap::new()),
            upload_lock: AsyncMutex::new(()),
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        session_id: &str,
        remote_path: &str,
    ) -> AppResult<EditStart> {
        let resolved = self.sessions.resolved_for(session_id).await?;
        let dir = self.store.dir().join("edits");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot create edit dir: {e}")))?;
        let name = remote_path
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .unwrap_or("remote-file");
        let local = dir.join(format!("{}-{name}", Uuid::new_v4()));
        let mut fs = remote::connect(&resolved).await?;
        let control = TransferControl::new(
            CancellationToken::new(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        );
        let result = fs
            .download(remote_path, &local, 0, &control, Arc::new(|_| {}))
            .await;
        let _ = fs.close().await;
        result?;

        let cancel = CancellationToken::new();
        {
            let mut active = self.active.lock().unwrap();
            if let Some(old) = active.remove(remote_path) {
                old.cancel.cancel();
            }
            active.insert(
                remote_path.to_string(),
                EditSession {
                    cancel: cancel.clone(),
                },
            );
        }

        let manager = self.clone();
        let remote = remote_path.to_string();
        let session = session_id.to_string();
        let path = local.clone();
        tokio::spawn(async move {
            manager.watch(session, remote, path, cancel).await;
        });

        Ok(EditStart {
            local_path: local.to_string_lossy().to_string(),
            remote_path: remote_path.to_string(),
        })
    }

    pub fn stop(&self, remote_path: &str) {
        if let Some(session) = self.active.lock().unwrap().remove(remote_path) {
            session.cancel.cancel();
        }
    }

    pub fn stop_all(&self) {
        let mut active = self.active.lock().unwrap();
        for (_, session) in active.drain() {
            session.cancel.cancel();
        }
    }

    async fn watch(
        self: Arc<Self>,
        session_id: String,
        remote_path: String,
        local_path: PathBuf,
        cancel: CancellationToken,
    ) {
        let mut baseline = metadata(&local_path).await;
        let mut pending: Option<(u64, Option<SystemTime>)> = None;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(Duration::from_millis(800)) => {}
            }
            let Some(meta) = metadata(&local_path).await else {
                continue;
            };
            let current = (meta.0, meta.1);
            let Some(base) = baseline else {
                baseline = Some(current);
                continue;
            };
            if current == base {
                pending = None;
                continue;
            }
            if pending == Some(current) {
                pending = None;
                if let Err(err) = self.upload(&session_id, &remote_path, &local_path).await {
                    tracing::warn!("remote edit upload failed: {}", err.message);
                } else {
                    baseline = metadata(&local_path).await;
                    let _ = self.app.emit(
                        "edit:uploaded",
                        EditUploaded {
                            local_path: local_path.to_string_lossy().to_string(),
                            remote_path: remote_path.clone(),
                            at: chrono::Utc::now().timestamp_millis(),
                        },
                    );
                }
            } else {
                pending = Some(current);
            }
        }
    }

    async fn upload(
        &self,
        session_id: &str,
        remote_path: &str,
        local_path: &Path,
    ) -> AppResult<()> {
        let _guard = self.upload_lock.lock().await;
        let resolved = self.sessions.resolved_for(session_id).await?;
        let mut fs = remote::connect(&resolved).await?;
        let control = TransferControl::new(
            CancellationToken::new(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        );
        let result = fs
            .upload(local_path, remote_path, 0, &control, Arc::new(|_| {}))
            .await;
        if result.is_ok() {
            if let Ok(meta) = tokio::fs::metadata(local_path).await {
                if let Ok(modified) = meta.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let _ = fs.set_mtime(remote_path, duration.as_secs() as i64).await;
                    }
                }
            }
        }
        let _ = fs.close().await;
        result.map(|_| ())
    }
}

async fn metadata(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    Some((meta.len(), meta.modified().ok()))
}
