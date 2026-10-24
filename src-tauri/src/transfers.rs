//! Transfer queue: scheduling, progress, pause/resume, retry and persistence.

use crate::error::{AppError, AppResult};
use crate::logs::LogBus;
use crate::models::{
    EnqueueRequest, TransferAction, TransferDirection, TransferItem, TransferStatus,
    TransferSummary, TransferTask,
};
use crate::remote::{self, ProgressCallback, RemoteFs, ResolvedConfig, TransferControl};
use crate::remote_path::join as join_remote;
use crate::session::SessionManager;
use crate::store::Store;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct TaskControl {
    cancel: CancellationToken,
    paused: Arc<AtomicBool>,
    speed_limit_bps: Arc<AtomicU64>,
}

struct QueuedFile {
    direction: TransferDirection,
    local_path: String,
    remote_path: String,
    size: u64,
    mtime: Option<i64>,
}

const QUEUE_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct QueuePayload {
    version: u32,
    tasks: Vec<TransferTask>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredQueue {
    Versioned(QueuePayload),
    Legacy(Vec<TransferTask>),
}

fn decode_queue(bytes: &[u8]) -> Option<Vec<TransferTask>> {
    match serde_json::from_slice::<StoredQueue>(bytes).ok()? {
        StoredQueue::Versioned(payload) => {
            if payload.version > QUEUE_VERSION {
                tracing::warn!("queue.json version {} is newer; ignoring", payload.version);
                None
            } else {
                Some(payload.tasks)
            }
        }
        StoredQueue::Legacy(tasks) => Some(tasks),
    }
}

fn set_local_mtime(path: &Path, timestamp: i64) -> std::io::Result<()> {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp.max(0) as u64);
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.set_modified(time)
}

fn parse_remote_mtime(value: &Option<String>) -> Option<i64> {
    value
        .as_deref()
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .map(|dt| dt.timestamp())
}

fn system_time_to_unix(value: std::time::SystemTime) -> Option<i64> {
    value
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs() as i64)
}

pub struct TransferManager {
    app: AppHandle,
    sessions: Arc<SessionManager>,
    store: Arc<Store>,
    logs: Arc<LogBus>,
    tasks: RwLock<Vec<TransferTask>>,
    controls: Mutex<HashMap<String, TaskControl>>,
    semaphore: RwLock<Arc<Semaphore>>,
    site_semaphores: Mutex<HashMap<String, (u8, Arc<Semaphore>)>>,
    last_emit: Mutex<HashMap<String, Instant>>,
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

const MAX_TREE_FILES: usize = 20_000;

/// Walk a local directory and collect every regular file plus the remote
/// directories that must exist before uploading.
async fn collect_upload(
    root_local: &str,
    root_remote: &str,
    files: &mut Vec<QueuedFile>,
    remote_dirs: &mut Vec<String>,
) -> AppResult<()> {
    let mut stack = vec![(PathBuf::from(root_local), root_remote.to_string())];
    while let Some((local, remote_path)) = stack.pop() {
        if files.len() > MAX_TREE_FILES {
            return Err(AppError::new(
                "TOO_MANY",
                "The folder contains too many files",
            ));
        }
        let meta = tokio::fs::symlink_metadata(&local).await.map_err(|e| {
            AppError::new("LOCAL_IO", format!("Cannot read {}: {e}", local.display()))
        })?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            remote_dirs.push(remote_path.clone());
            let mut reader = tokio::fs::read_dir(&local).await.map_err(|e| {
                AppError::new("LOCAL_IO", format!("Cannot list {}: {e}", local.display()))
            })?;
            while let Some(entry) = reader.next_entry().await.map_err(|e| {
                AppError::new("LOCAL_IO", format!("Cannot read {}: {e}", local.display()))
            })? {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "." || name == ".." {
                    continue;
                }
                stack.push((entry.path(), join_remote(&remote_path, &name)));
            }
        } else {
            files.push(QueuedFile {
                direction: TransferDirection::Upload,
                local_path: local.to_string_lossy().to_string(),
                remote_path,
                size: meta.len(),
                mtime: meta.modified().ok().and_then(system_time_to_unix),
            });
        }
    }
    Ok(())
}

/// Walk a remote directory and collect every file plus the local directories
/// that must be created before downloading.
async fn collect_download(
    fs: &mut Box<dyn RemoteFs>,
    root_remote: &str,
    root_local: &str,
    files: &mut Vec<QueuedFile>,
    local_dirs: &mut Vec<PathBuf>,
) -> AppResult<()> {
    let mut stack = vec![(
        crate::remote_path::normalize(root_remote),
        PathBuf::from(root_local),
    )];
    while let Some((remote_path, local_path)) = stack.pop() {
        if files.len() > MAX_TREE_FILES {
            return Err(AppError::new(
                "TOO_MANY",
                "The folder contains too many files",
            ));
        }
        local_dirs.push(local_path.clone());
        let entries = fs.list(&remote_path).await?;
        for entry in entries {
            let child_local = local_path.join(&entry.name);
            if entry.is_dir {
                if entry.is_symlink {
                    continue;
                }
                stack.push((entry.path, child_local));
            } else {
                files.push(QueuedFile {
                    direction: TransferDirection::Download,
                    local_path: child_local.to_string_lossy().to_string(),
                    remote_path: entry.path,
                    size: entry.size,
                    mtime: parse_remote_mtime(&entry.modified),
                });
            }
        }
    }
    Ok(())
}

async fn expand_items(
    resolved: &ResolvedConfig,
    items: Vec<TransferItem>,
) -> AppResult<Vec<QueuedFile>> {
    let mut files = Vec::new();
    let mut remote_dirs: Vec<String> = Vec::new();
    let mut local_dirs: Vec<PathBuf> = Vec::new();
    let mut remote: Option<Box<dyn RemoteFs>> = None;

    for item in items {
        match item.direction {
            TransferDirection::Upload => {
                let meta = tokio::fs::metadata(&item.local_path).await.ok();
                if meta.as_ref().is_some_and(|m| m.is_dir()) {
                    collect_upload(
                        &item.local_path,
                        &item.remote_path,
                        &mut files,
                        &mut remote_dirs,
                    )
                    .await?;
                } else {
                    files.push(QueuedFile {
                        direction: TransferDirection::Upload,
                        local_path: item.local_path,
                        remote_path: item.remote_path,
                        size: meta
                            .as_ref()
                            .map(|m| m.len())
                            .unwrap_or(item.size.unwrap_or(0)),
                        mtime: meta
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(system_time_to_unix),
                    });
                }
            }
            TransferDirection::Download => {
                if item.is_dir {
                    if remote.is_none() {
                        remote = Some(remote::connect(resolved).await?);
                    }
                    let fs = remote.as_mut().expect("connection");
                    collect_download(
                        fs,
                        &item.remote_path,
                        &item.local_path,
                        &mut files,
                        &mut local_dirs,
                    )
                    .await?;
                } else {
                    if remote.is_none() {
                        remote = Some(remote::connect(resolved).await?);
                    }
                    let entry = remote
                        .as_mut()
                        .expect("connection")
                        .stat(&item.remote_path)
                        .await
                        .ok();
                    files.push(QueuedFile {
                        direction: TransferDirection::Download,
                        local_path: item.local_path,
                        remote_path: item.remote_path,
                        size: entry
                            .as_ref()
                            .map(|entry| entry.size)
                            .unwrap_or(item.size.unwrap_or(0)),
                        mtime: entry
                            .as_ref()
                            .and_then(|entry| parse_remote_mtime(&entry.modified)),
                    });
                }
            }
        }
    }

    for dir in &local_dirs {
        tokio::fs::create_dir_all(dir).await.map_err(|e| {
            AppError::new("LOCAL_IO", format!("Cannot create {}: {e}", dir.display()))
        })?;
    }

    if !remote_dirs.is_empty() {
        if remote.is_none() {
            remote = Some(remote::connect(resolved).await?);
        }
        remote_dirs.sort_by_key(|path| path.matches('/').count());
        remote_dirs.dedup();
        let fs = remote.as_mut().expect("connection");
        for dir in &remote_dirs {
            if let Err(err) = fs.mkdir(dir).await {
                // An existing directory is fine; a real permission problem
                // surfaces on the first upload with a clearer error.
                tracing::debug!("mkdir {dir}: {}", err.message);
            }
        }
    }

    if let Some(mut fs) = remote {
        let _ = fs.close().await;
    }
    Ok(files)
}

impl TransferManager {
    pub fn new(
        app: AppHandle,
        sessions: Arc<SessionManager>,
        store: Arc<Store>,
        logs: Arc<LogBus>,
    ) -> Arc<Self> {
        let concurrency = store.settings().concurrent_transfers.clamp(1, 10) as usize;
        let manager = Arc::new(Self {
            app,
            sessions,
            store,
            logs,
            tasks: RwLock::new(Vec::new()),
            controls: Mutex::new(HashMap::new()),
            semaphore: RwLock::new(Arc::new(Semaphore::new(concurrency))),
            site_semaphores: Mutex::new(HashMap::new()),
            last_emit: Mutex::new(HashMap::new()),
        });
        manager.restore_queue();
        manager
    }

    fn restore_queue(&self) {
        let path = self.store.dir().join("queue.json");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Some(saved) = decode_queue(&bytes) else {
            return;
        };
        let mut controls = self.controls.lock().unwrap();
        let mut tasks = self.tasks.write().unwrap();
        for mut task in saved {
            if task.status.is_terminal() || task.site_id.is_none() {
                continue;
            }
            task.session_id.clear();
            task.status = TransferStatus::Paused;
            task.speed_bps = 0;
            task.error = Some("Restored from the previous session".into());
            task.error_code = Some("RESTORED".into());
            controls.insert(
                task.id.clone(),
                TaskControl {
                    cancel: CancellationToken::new(),
                    paused: Arc::new(AtomicBool::new(true)),
                    speed_limit_bps: Arc::new(AtomicU64::new(0)),
                },
            );
            tasks.push(task);
        }
        tracing::info!("restored {} transfer(s) from queue.json", tasks.len());
    }

    fn save_queue(&self) {
        let tasks: Vec<TransferTask> = self
            .tasks
            .read()
            .unwrap()
            .iter()
            .filter(|task| !task.status.is_terminal())
            .cloned()
            .collect();
        if let Ok(bytes) = serde_json::to_vec_pretty(&QueuePayload {
            version: QUEUE_VERSION,
            tasks,
        }) {
            let _ = self.store.write_aux("queue.json", &bytes);
        }
    }

    /// Rebind restored paused tasks to a freshly connected session and start them.
    pub fn attach_session(self: &Arc<Self>, info: &crate::models::SessionInfo) {
        let Some(site_id) = info.site_id.as_deref() else {
            return;
        };
        let mut start = Vec::new();
        {
            let mut tasks = self.tasks.write().unwrap();
            let mut controls = self.controls.lock().unwrap();
            for task in tasks.iter_mut() {
                if task.site_id.as_deref() != Some(site_id)
                    || task.status != TransferStatus::Paused
                    || task.error_code.as_deref() != Some("RESTORED")
                {
                    continue;
                }
                task.session_id = info.session_id.clone();
                task.site_name = info.name.clone();
                task.status = TransferStatus::Queued;
                task.error = None;
                task.error_code = None;
                controls.insert(
                    task.id.clone(),
                    TaskControl {
                        cancel: CancellationToken::new(),
                        paused: Arc::new(AtomicBool::new(false)),
                        speed_limit_bps: Arc::new(AtomicU64::new(0)),
                    },
                );
                start.push(task.id.clone());
            }
        }
        if start.is_empty() {
            return;
        }
        self.save_queue();
        for id in start {
            self.emit(&id);
            let manager = self.clone();
            tokio::spawn(async move {
                manager.run(id).await;
            });
        }
    }

    pub fn set_concurrency(&self, value: u8) {
        let value = value.clamp(1, 10) as usize;
        *self.semaphore.write().unwrap() = Arc::new(Semaphore::new(value));
    }

    pub fn list(&self) -> Vec<TransferTask> {
        self.tasks.read().unwrap().clone()
    }

    pub fn summary(&self) -> TransferSummary {
        let tasks = self.tasks.read().unwrap();
        let mut summary = TransferSummary {
            active: 0,
            queued: 0,
            completed: 0,
            failed: 0,
            total_speed_bps: 0,
        };
        for task in tasks.iter() {
            match task.status {
                TransferStatus::Active | TransferStatus::Connecting => summary.active += 1,
                TransferStatus::Queued | TransferStatus::Paused => summary.queued += 1,
                TransferStatus::Completed => summary.completed += 1,
                TransferStatus::Failed | TransferStatus::Cancelled => summary.failed += 1,
            }
            if matches!(
                task.status,
                TransferStatus::Active | TransferStatus::Connecting
            ) {
                summary.total_speed_bps += task.speed_bps;
            }
        }
        summary
    }

    pub async fn enqueue(
        self: &Arc<Self>,
        request: EnqueueRequest,
    ) -> AppResult<Vec<TransferTask>> {
        if request.items.is_empty() {
            return Err(AppError::invalid("No files were selected"));
        }
        let resolved = self.sessions.resolved_for(&request.session_id).await?;
        let settings = self.store.settings();
        let files = expand_items(&resolved, request.items).await?;
        if files.is_empty() {
            self.logs.info(
                "transfer",
                "Folder prepared; it contains no files to transfer",
            );
            return Ok(Vec::new());
        }
        let mut created = Vec::new();

        for item in files {
            let name = item
                .remote_path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|n| !n.is_empty())
                .unwrap_or("download")
                .to_string();
            let id = Uuid::new_v4().to_string();
            let task = TransferTask {
                id: id.clone(),
                session_id: request.session_id.clone(),
                site_id: resolved.site_id.clone(),
                site_name: resolved.name.clone(),
                direction: item.direction,
                local_path: item.local_path,
                remote_path: item.remote_path,
                name,
                size: item.size,
                transferred: 0,
                resumed_from: 0,
                mtime: item.mtime,
                status: TransferStatus::Queued,
                error: None,
                error_code: None,
                speed_bps: 0,
                created_at: now_ms(),
                started_at: None,
                finished_at: None,
            };
            let limit = match item.direction {
                TransferDirection::Download => settings.download_limit_kbps,
                TransferDirection::Upload => settings.upload_limit_kbps,
            };
            let control = TaskControl {
                cancel: CancellationToken::new(),
                paused: Arc::new(AtomicBool::new(false)),
                speed_limit_bps: Arc::new(AtomicU64::new(u64::from(limit) * 1024)),
            };
            self.controls.lock().unwrap().insert(id.clone(), control);
            self.tasks.write().unwrap().push(task.clone());
            self.emit_task(&task);
            created.push(task);
            let manager = self.clone();
            let task_id = id;
            tokio::spawn(async move {
                manager.run(task_id).await;
            });
        }
        self.save_queue();
        Ok(created)
    }

    async fn run(self: Arc<Self>, task_id: String) {
        let Some(task_snapshot) = self.get_task(&task_id) else {
            return;
        };
        let Some((cancel, paused, limit)) = self.control_parts(&task_id) else {
            return;
        };
        let ctl = TransferControl::new(cancel.clone(), paused.clone(), limit.clone());

        let resolved = match self.sessions.resolved_for(&task_snapshot.session_id).await {
            Ok(config) => config,
            Err(err) => {
                self.fail(&task_id, &err.code, &err.message);
                return;
            }
        };

        let _site_permit = if let Some(site) = task_snapshot.site_id.clone() {
            let limit = resolved.config.max_connections.clamp(1, 10);
            let semaphore = {
                let mut map = self.site_semaphores.lock().unwrap();
                match map.get(&site) {
                    Some((current, semaphore)) if *current == limit => semaphore.clone(),
                    _ => {
                        let semaphore = Arc::new(Semaphore::new(limit as usize));
                        map.insert(site, (limit, semaphore.clone()));
                        semaphore
                    }
                }
            };
            let permit = tokio::select! {
                _ = cancel.cancelled() => {
                    self.finish_cancelled(&task_id);
                    return;
                }
                permit = semaphore.acquire_owned() => permit,
            };
            match permit {
                Ok(permit) => Some(permit),
                Err(_) => {
                    self.fail(&task_id, "QUEUE", "Site transfer queue was closed");
                    return;
                }
            }
        } else {
            None
        };

        let permit = {
            let semaphore = self.semaphore.read().unwrap().clone();
            tokio::select! {
                _ = cancel.cancelled() => {
                    self.finish_cancelled(&task_id);
                    return;
                }
                permit = semaphore.acquire_owned() => permit,
            }
        };
        let Ok(_permit) = permit else {
            self.fail(&task_id, "QUEUE", "Transfer queue was closed");
            return;
        };
        self.update_status(&task_id, TransferStatus::Connecting, None);
        let remote = match remote::connect(&resolved).await {
            Ok(fs) => fs,
            Err(err) => {
                self.fail(&task_id, &err.code, &err.message);
                return;
            }
        };
        let mut remote = remote;

        // Resolve size and resume offset. Resuming is only allowed when this
        // task itself has an in-progress record (paused mid-transfer or
        // restored from queue.json); a fresh task always overwrites the
        // existing remote/local file instead of trusting an unrelated partial.
        let mut size = task_snapshot.size;
        let mut resume_from = 0u64;
        let settings = self.store.settings();
        let had_progress = task_snapshot.transferred > 0;
        match task_snapshot.direction {
            TransferDirection::Download => {
                if let Ok(entry) = remote.stat(&task_snapshot.remote_path).await {
                    if entry.is_dir {
                        self.fail(&task_id, "REMOTE", "Cannot download a directory");
                        let _ = remote.close().await;
                        return;
                    }
                    size = entry.size;
                }
                if settings.resume_transfers && had_progress {
                    if let Ok(meta) = tokio::fs::metadata(&task_snapshot.local_path).await {
                        let existing = meta.len();
                        if existing > 0 && (size == 0 || existing < size) {
                            resume_from = existing;
                        }
                    }
                }
            }
            TransferDirection::Upload => {
                if let Ok(meta) = tokio::fs::metadata(&task_snapshot.local_path).await {
                    size = meta.len();
                }
                if settings.resume_transfers && had_progress && size > 0 {
                    if let Ok(entry) = remote.stat(&task_snapshot.remote_path).await {
                        if !entry.is_dir && entry.size > 0 && entry.size < size {
                            resume_from = entry.size;
                        }
                    }
                }
            }
        }

        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.size = size;
                task.resumed_from = resume_from;
                task.transferred = resume_from;
                task.status = TransferStatus::Active;
                task.started_at = Some(now_ms());
            }
        }
        self.emit(&task_id);

        if size > 0 && resume_from >= size {
            // Already complete.
            self.complete(&task_id, size);
            let _ = remote.close().await;
            return;
        }

        self.logs.info(
            "transfer",
            format!(
                "{} {} -> {}",
                match task_snapshot.direction {
                    TransferDirection::Download => "Downloading",
                    TransferDirection::Upload => "Uploading",
                },
                task_snapshot.remote_path,
                task_snapshot.local_path
            ),
        );

        let manager = self.clone();
        let progress_id = task_id.clone();
        let progress: ProgressCallback = Arc::new(move |transferred| {
            manager.progress(&progress_id, transferred);
        });

        let result = match task_snapshot.direction {
            TransferDirection::Download => {
                let local = PathBuf::from(&task_snapshot.local_path);
                remote
                    .download(
                        &task_snapshot.remote_path,
                        &local,
                        resume_from,
                        &ctl,
                        progress,
                    )
                    .await
            }
            TransferDirection::Upload => {
                let local = PathBuf::from(&task_snapshot.local_path);
                remote
                    .upload(
                        &local,
                        &task_snapshot.remote_path,
                        resume_from,
                        &ctl,
                        progress,
                    )
                    .await
            }
        };

        if result.is_ok() && settings.preserve_timestamps {
            if let Some(timestamp) = task_snapshot.mtime {
                match task_snapshot.direction {
                    TransferDirection::Download => {
                        if let Err(err) =
                            set_local_mtime(Path::new(&task_snapshot.local_path), timestamp)
                        {
                            tracing::warn!(
                                "cannot preserve local mtime for {}: {err}",
                                task_snapshot.local_path
                            );
                        }
                    }
                    TransferDirection::Upload => {
                        if let Err(err) = remote
                            .set_mtime(&task_snapshot.remote_path, timestamp)
                            .await
                        {
                            tracing::debug!(
                                "server cannot preserve mtime for {}: {}",
                                task_snapshot.remote_path,
                                err.message
                            );
                        }
                    }
                }
            }
        }
        let _ = remote.close().await;

        match result {
            Ok(total) => {
                self.complete(&task_id, total.max(size));
            }
            Err(err) if err.code == "CANCELLED" => {
                self.finish_cancelled(&task_id);
            }
            Err(err) => {
                self.fail(&task_id, &err.code, &err.message);
            }
        }
    }

    fn control_parts(
        &self,
        task_id: &str,
    ) -> Option<(CancellationToken, Arc<AtomicBool>, Arc<AtomicU64>)> {
        let map = self.controls.lock().unwrap();
        map.get(task_id).map(|c| {
            (
                c.cancel.clone(),
                c.paused.clone(),
                c.speed_limit_bps.clone(),
            )
        })
    }

    fn get_task(&self, task_id: &str) -> Option<TransferTask> {
        self.tasks
            .read()
            .unwrap()
            .iter()
            .find(|t| t.id == task_id)
            .cloned()
    }

    fn progress(&self, task_id: &str, transferred: u64) {
        {
            let mut tasks = self.tasks.write().unwrap();
            let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) else {
                return;
            };
            task.transferred = transferred;
            let elapsed = task
                .started_at
                .map(|start| (now_ms() - start).max(1) as f64 / 1000.0)
                .unwrap_or(1.0);
            task.speed_bps =
                (((transferred.saturating_sub(task.resumed_from)) as f64) / elapsed) as u64;
        }
        let mut last = self.last_emit.lock().unwrap();
        let now = Instant::now();
        let due = last
            .get(task_id)
            .map(|t| now.duration_since(*t) >= Duration::from_millis(180))
            .unwrap_or(true);
        if due {
            last.insert(task_id.to_string(), now);
            drop(last);
            self.emit(task_id);
        }
    }

    fn update_status(&self, task_id: &str, status: TransferStatus, error: Option<AppError>) {
        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = status;
                if let Some(err) = error {
                    task.error = Some(err.message);
                    task.error_code = Some(err.code);
                }
            }
        }
        self.emit(task_id);
        self.save_queue();
    }

    fn complete(&self, task_id: &str, total: u64) {
        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TransferStatus::Completed;
                task.transferred = total.max(task.transferred);
                task.size = task.size.max(task.transferred);
                task.speed_bps = 0;
                task.finished_at = Some(now_ms());
                task.error = None;
                task.error_code = None;
            }
        }
        self.controls.lock().unwrap().remove(task_id);
        self.emit(task_id);
        self.save_queue();
        if let Some(task) = self.get_task(task_id) {
            self.logs
                .info("transfer", format!("Completed {}", task.name));
        }
    }

    fn fail(&self, task_id: &str, code: &str, message: &str) {
        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TransferStatus::Failed;
                task.error = Some(message.to_string());
                task.error_code = Some(code.to_string());
                task.speed_bps = 0;
                task.finished_at = Some(now_ms());
            }
        }
        self.controls.lock().unwrap().remove(task_id);
        self.emit(task_id);
        self.save_queue();
        self.logs
            .error("transfer", format!("Transfer failed ({code}): {message}"));
    }

    fn finish_cancelled(&self, task_id: &str) {
        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                if matches!(
                    task.status,
                    TransferStatus::Completed | TransferStatus::Failed
                ) {
                    return;
                }
                task.status = TransferStatus::Cancelled;
                task.speed_bps = 0;
                task.finished_at = Some(now_ms());
            }
        }
        self.controls.lock().unwrap().remove(task_id);
        self.emit(task_id);
        self.save_queue();
    }

    pub fn action(self: &Arc<Self>, task_id: &str, action: TransferAction) -> AppResult<()> {
        let task = self
            .get_task(task_id)
            .ok_or_else(|| AppError::not_found("Transfer not found"))?;
        match action {
            TransferAction::Pause => {
                if !matches!(
                    task.status,
                    TransferStatus::Active | TransferStatus::Connecting
                ) {
                    return Err(AppError::invalid("This transfer cannot be paused"));
                }
                if let Some(control) = self.controls.lock().unwrap().get(task_id) {
                    control.paused.store(true, Ordering::Relaxed);
                }
                self.update_status(task_id, TransferStatus::Paused, None);
            }
            TransferAction::Resume => {
                if task.status != TransferStatus::Paused {
                    return Err(AppError::invalid("This transfer is not paused"));
                }
                if let Some(control) = self.controls.lock().unwrap().get(task_id) {
                    control.paused.store(false, Ordering::Relaxed);
                }
                self.update_status(task_id, TransferStatus::Active, None);
            }
            TransferAction::Cancel => {
                if task.status.is_terminal() {
                    return Ok(());
                }
                if let Some(control) = self.controls.lock().unwrap().get(task_id) {
                    control.paused.store(false, Ordering::Relaxed);
                    control.cancel.cancel();
                }
                if task.status == TransferStatus::Queued {
                    self.finish_cancelled(task_id);
                } else {
                    self.update_status(task_id, TransferStatus::Cancelled, None);
                }
            }
            TransferAction::Retry => {
                if !task.status.is_terminal() {
                    return Err(AppError::invalid("Only finished transfers can be retried"));
                }
                self.retry(task_id)?;
            }
            TransferAction::Remove => {
                if !task.status.is_terminal() {
                    return Err(AppError::invalid("Pause or cancel the transfer first"));
                }
                self.tasks.write().unwrap().retain(|t| t.id != task_id);
                self.controls.lock().unwrap().remove(task_id);
                let _ = self.app.emit("transfer:removed", task_id.to_string());
                self.save_queue();
            }
        }
        Ok(())
    }

    fn retry(self: &Arc<Self>, task_id: &str) -> AppResult<()> {
        let task = self
            .get_task(task_id)
            .ok_or_else(|| AppError::not_found("Transfer not found"))?;
        let settings = self.store.settings();
        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TransferStatus::Queued;
                task.transferred = 0;
                task.resumed_from = 0;
                task.speed_bps = 0;
                task.error = None;
                task.error_code = None;
                task.started_at = None;
                task.finished_at = None;
            }
        }
        let limit = match task.direction {
            TransferDirection::Download => settings.download_limit_kbps,
            TransferDirection::Upload => settings.upload_limit_kbps,
        };
        self.controls.lock().unwrap().insert(
            task_id.to_string(),
            TaskControl {
                cancel: CancellationToken::new(),
                paused: Arc::new(AtomicBool::new(false)),
                speed_limit_bps: Arc::new(AtomicU64::new(u64::from(limit) * 1024)),
            },
        );
        self.emit(task_id);
        self.save_queue();
        let manager = self.clone();
        let id = task_id.to_string();
        tokio::spawn(async move {
            manager.run(id).await;
        });
        Ok(())
    }

    pub fn clear_finished(&self) {
        let removed: Vec<String> = {
            let mut tasks = self.tasks.write().unwrap();
            let removed: Vec<String> = tasks
                .iter()
                .filter(|t| t.status.is_terminal())
                .map(|t| t.id.clone())
                .collect();
            tasks.retain(|t| !t.status.is_terminal());
            removed
        };
        let mut controls = self.controls.lock().unwrap();
        for id in &removed {
            controls.remove(id);
        }
        drop(controls);
        let _ = self.app.emit("transfers:cleared", removed);
        self.save_queue();
    }

    fn emit(&self, task_id: &str) {
        if let Some(task) = self.get_task(task_id) {
            self.emit_task(&task);
        }
    }

    fn emit_task(&self, task: &TransferTask) {
        let _ = self.app.emit("transfer:update", task.clone());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TransferDirection, TransferStatus};

    #[test]
    fn queue_schema_accepts_v1_and_v2() {
        assert_eq!(decode_queue(b"[]").unwrap().len(), 0);
        assert_eq!(
            decode_queue(br#"{"version":2,"tasks":[]}"#).unwrap().len(),
            0
        );
        assert!(decode_queue(br#"{"version":3,"tasks":[]}"#).is_none());
        assert!(decode_queue(b"null").is_none());
    }
    #[test]
    fn timestamp_helpers_roundtrip() {
        assert_eq!(
            parse_remote_mtime(&Some("2023-09-19T11:12:03Z".into())),
            Some(1695121923)
        );
        assert_eq!(parse_remote_mtime(&Some("not-a-date".into())), None);
        assert_eq!(parse_remote_mtime(&None), None);

        let path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test-mtime.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
        set_local_mtime(&path, 1_600_000_000).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let unix = modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!((unix as i64 - 1_600_000_000).abs() <= 2);
        std::fs::remove_file(&path).ok();
    }
    #[test]
    fn queue_task_roundtrips_with_site_id() {
        let task = TransferTask {
            id: "t1".into(),
            session_id: String::new(),
            site_id: Some("site-1".into()),
            site_name: "Example".into(),
            direction: TransferDirection::Download,
            local_path: "C:\\tmp\\a.txt".into(),
            remote_path: "/a.txt".into(),
            name: "a.txt".into(),
            size: 12,
            transferred: 4,
            resumed_from: 4,
            mtime: None,
            status: TransferStatus::Paused,
            error: Some("Restored from the previous session".into()),
            error_code: Some("RESTORED".into()),
            speed_bps: 0,
            created_at: 1,
            started_at: None,
            finished_at: None,
        };
        let json = serde_json::to_string(&task).unwrap();
        let restored: TransferTask = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.site_id.as_deref(), Some("site-1"));
        assert_eq!(restored.status, TransferStatus::Paused);
        assert_eq!(restored.resumed_from, 4);
    }
}
