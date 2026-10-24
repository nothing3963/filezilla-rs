use crate::error::{AppError, AppResult};
use crate::local;
use crate::logs::LogBus;
use crate::models::{
    AppInfo, AppSettings, ConnectRequest, EnqueueRequest, KnownHost, LocalListing, RemoteListing,
    SessionInfo, Site, SiteGroup, SiteInput, SitesSnapshot, TransferAction, TransferSummary,
    TransferTask,
};
use crate::remote_path;
use crate::state::AppState;
use tauri::{Manager, State};

fn now_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: "FileZilla Modern".into(),
        version: now_version().into(),
        tauri_version: tauri::VERSION.into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        config_dir: state.store.dir().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn managed_policy(state: State<'_, AppState>) -> crate::defaults::ManagedPolicy {
    (*state.policy).clone()
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> AppSettings {
    state.store.settings()
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, settings: AppSettings) -> AppResult<AppSettings> {
    let saved = state.store.save_settings(settings)?;
    state.transfers.set_concurrency(saved.concurrent_transfers);
    Ok(saved)
}

#[tauri::command]
pub fn sites_get(state: State<'_, AppState>) -> SitesSnapshot {
    state.store.snapshot()
}

#[tauri::command]
pub fn site_save(state: State<'_, AppState>, input: SiteInput) -> AppResult<Site> {
    let site = state.store.save_site(input)?;
    state
        .logs
        .info("sites", format!("Saved site {}", site.name));
    Ok(site)
}

#[tauri::command]
pub fn site_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_site(&id)
}

#[tauri::command]
pub fn site_duplicate(state: State<'_, AppState>, id: String) -> AppResult<Site> {
    state.store.duplicate_site(&id)
}

#[tauri::command]
pub fn sites_import(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<crate::site_xml::ImportOutcome> {
    crate::site_xml::import_file(&state.store, std::path::Path::new(&path))
}

#[tauri::command]
pub fn sites_detect_auto(state: State<'_, AppState>) -> Option<crate::site_xml::DetectedSites> {
    crate::site_xml::detect(state.policy.config_dir.as_deref()).map(|path| {
        crate::site_xml::DetectedSites {
            source: path.display().to_string(),
        }
    })
}

#[tauri::command]
pub fn sites_import_auto(
    state: State<'_, AppState>,
) -> AppResult<Option<crate::site_xml::AutoImportOutcome>> {
    let outcome =
        crate::site_xml::import_detected(&state.store, state.policy.config_dir.as_deref())?;
    if let Some(result) = &outcome {
        state.logs.info(
            "sites",
            format!(
                "Imported {} site(s) from {}",
                result.imported,
                result.sources.join(", ")
            ),
        );
    }
    Ok(outcome)
}

#[tauri::command]
pub fn sites_export(
    state: State<'_, AppState>,
    path: String,
    include_passwords: bool,
) -> AppResult<crate::site_xml::ExportOutcome> {
    crate::site_xml::export_file(&state.store, std::path::Path::new(&path), include_passwords)
}
#[tauri::command]
pub fn group_create(
    state: State<'_, AppState>,
    name: String,
    parent_id: Option<String>,
) -> AppResult<SiteGroup> {
    state.store.create_group(&name, parent_id)
}

#[tauri::command]
pub fn group_rename(state: State<'_, AppState>, id: String, name: String) -> AppResult<SiteGroup> {
    state.store.rename_group(&id, &name)
}

#[tauri::command]
pub fn group_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.store.delete_group(&id)
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    request: ConnectRequest,
) -> AppResult<SessionInfo> {
    let info = state.sessions.connect(request).await?;
    state.transfers.attach_session(&info);
    state.logs.info(
        "session",
        format!("Connected to {} ({})", info.name, info.host),
    );
    Ok(info)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.sessions.disconnect(&session_id).await?;
    state
        .logs
        .info("session", format!("Disconnected {session_id}"));
    Ok(())
}

#[tauri::command]
pub async fn remote_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<RemoteListing> {
    let path = remote_path::normalize(&path);
    let entries = state.sessions.list_remote(&session_id, &path).await?;
    Ok(RemoteListing {
        parent: remote_path::parent(&path),
        path,
        entries,
    })
}

#[tauri::command]
pub async fn remote_stat(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<crate::models::RemoteEntry> {
    state.sessions.stat(&session_id, &path).await
}

#[tauri::command]
pub async fn remote_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<()> {
    state.sessions.mkdir(&session_id, &path).await?;
    state.logs.info("remote", format!("Created folder {path}"));
    Ok(())
}

#[tauri::command]
pub async fn remote_rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> AppResult<()> {
    state.sessions.rename(&session_id, &from, &to).await?;
    state.logs.info("remote", format!("Renamed {from} to {to}"));
    Ok(())
}

#[tauri::command]
pub async fn remote_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> AppResult<()> {
    state.sessions.chmod(&session_id, &path, mode).await?;
    state.logs.info(
        "remote",
        format!("Changed permissions of {path} to {mode:o}"),
    );
    Ok(())
}

#[tauri::command]
pub async fn remote_delete(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    recursive: bool,
) -> AppResult<()> {
    let path = remote_path::normalize(&path);
    if path == "/" {
        return Err(AppError::invalid("Refusing to delete the root directory"));
    }
    if recursive {
        delete_tree(&state, &session_id, &path).await?;
    } else {
        let entry = state.sessions.stat(&session_id, &path).await?;
        if entry.is_dir {
            state.sessions.remove_dir(&session_id, &path).await?;
        } else {
            state.sessions.remove_file(&session_id, &path).await?;
        }
    }
    state.logs.info("remote", format!("Deleted {path}"));
    Ok(())
}

fn delete_tree<'a>(
    state: &'a AppState,
    session_id: &'a str,
    path: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut stack = vec![path.to_string()];
        let mut files: Vec<String> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        while let Some(dir) = stack.pop() {
            if dirs.len() > 20_000 {
                return Err(AppError::new("REMOTE", "Directory tree is too large"));
            }
            let entries = state.sessions.list_remote(session_id, &dir).await?;
            for entry in entries {
                if entry.is_dir && !entry.is_symlink {
                    stack.push(entry.path);
                } else {
                    files.push(entry.path);
                }
            }
            dirs.push(dir);
        }
        for file in files {
            state.sessions.remove_file(session_id, &file).await?;
        }
        for dir in dirs.iter().rev() {
            state.sessions.remove_dir(session_id, dir).await?;
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn local_list(
    state: State<'_, AppState>,
    path: Option<String>,
    show_hidden: Option<bool>,
) -> AppResult<LocalListing> {
    let show_hidden = show_hidden.unwrap_or(state.store.settings().show_hidden_files);
    local::list(path.as_deref(), show_hidden).await
}

#[tauri::command]
pub async fn local_mkdir(path: String) -> AppResult<()> {
    local::create_dir(&path).await
}

#[tauri::command]
pub async fn local_rename(from: String, to: String) -> AppResult<()> {
    local::rename(&from, &to).await
}

#[tauri::command]
pub async fn local_delete(path: String) -> AppResult<()> {
    local::delete(&path).await
}

/// Recursively search a remote tree by file name.  Bounded by `max_results`
/// and depth to keep a misbehaving server from exhausting the UI.
#[tauri::command]
pub async fn search_remote(
    state: State<'_, AppState>,
    session_id: String,
    root: String,
    query: String,
    max_results: Option<usize>,
) -> AppResult<Vec<crate::models::RemoteEntry>> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Err(AppError::invalid("Search query is required"));
    }
    let limit = max_results.unwrap_or(200).clamp(1, 1000);
    let start = root.clone();
    let mut pending = vec![(root, 0usize)];
    let mut results = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        if results.len() >= limit {
            break;
        }
        let entries = match state.sessions.list_remote(&session_id, &directory).await {
            Ok(entries) => entries,
            Err(err) if directory == start => return Err(err),
            Err(err) => {
                tracing::debug!("search skipped {directory}: {}", err.message);
                continue;
            }
        };
        for entry in entries {
            if entry.name.to_lowercase().contains(&needle) {
                results.push(entry.clone());
                if results.len() >= limit {
                    break;
                }
            }
            if entry.is_dir && !entry.is_symlink && depth < 8 {
                pending.push((entry.path, depth + 1));
            }
        }
    }
    Ok(results)
}
#[tauri::command]
pub fn ssh_hostkey_resolve(id: String, accepted: bool) -> bool {
    crate::hostkey::resolve(&id, accepted)
}
#[tauri::command]
pub async fn remote_edit_start(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
) -> AppResult<crate::editor::EditStart> {
    state.edits.start(&session_id, &remote_path).await
}

#[tauri::command]
pub fn remote_edit_stop(state: State<'_, AppState>, remote_path: String) {
    state.edits.stop(&remote_path);
}
#[tauri::command]
pub async fn enqueue(
    state: State<'_, AppState>,
    request: EnqueueRequest,
) -> AppResult<Vec<TransferTask>> {
    let tasks = state.transfers.enqueue(request).await?;
    state
        .logs
        .info("transfer", format!("Queued {} transfer(s)", tasks.len()));
    Ok(tasks)
}

#[tauri::command]
pub fn transfers_list(state: State<'_, AppState>) -> Vec<TransferTask> {
    state.transfers.list()
}

#[tauri::command]
pub fn transfer_summary(state: State<'_, AppState>) -> TransferSummary {
    state.transfers.summary()
}

#[tauri::command]
pub fn transfer_action(
    state: State<'_, AppState>,
    task_id: String,
    action: TransferAction,
) -> AppResult<()> {
    state.transfers.action(&task_id, action)
}

#[tauri::command]
pub fn transfer_clear(state: State<'_, AppState>) {
    state.transfers.clear_finished();
}

#[tauri::command]
pub fn logs_history(state: State<'_, AppState>) -> Vec<crate::models::LogEntry> {
    state.logs.history()
}

#[tauri::command]
pub fn logs_clear(state: State<'_, AppState>) {
    state.logs.clear();
}

#[tauri::command]
pub fn known_hosts_list(state: State<'_, AppState>) -> Vec<KnownHost> {
    state.store.known_hosts()
}

#[tauri::command]
pub fn known_host_forget(state: State<'_, AppState>, host: String) -> AppResult<()> {
    state.store.forget_host(&host)
}

#[tauri::command]
pub fn log_client(state: State<'_, AppState>, level: String, scope: String, message: String) {
    let bus: &LogBus = &state.logs;
    match level.as_str() {
        "warn" => bus.warn(&scope, message),
        "error" => bus.error(&scope, message),
        "debug" => bus.debug(&scope, message),
        _ => bus.info(&scope, message),
    }
}

/// Re-apply the window backdrop when the theme changes.
///
/// The webview's `prefers-color-scheme` must stay under OS control: forcing the
/// window theme makes WebView2 report the forced value, so "follow system"
/// would read back the previously selected light/dark mode.
#[tauri::command]
pub async fn apply_window_theme(app: tauri::AppHandle, dark: bool, mica: bool) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        if let Some(window) = app.get_webview_window("main") {
            if mica {
                let _ = window_vibrancy::apply_mica(&window, Some(dark));
            } else {
                let _ = window_vibrancy::clear_mica(&window);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, dark, mica);
    }
    Ok(())
}
