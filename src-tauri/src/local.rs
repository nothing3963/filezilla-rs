//! Local file system adapter.
//!
//! The UI is a two-pane file manager: everything here backs the left pane.
//! Paths are native Windows paths and never normalised as POSIX.

use crate::error::{AppError, AppResult};
use crate::models::{LocalEntry, LocalKind, LocalListing};
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Local> = time.into();
    Some(dt.to_rfc3339())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn is_hidden(path: &Path, name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
            const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
            let attrs = meta.file_attributes();
            return attrs & FILE_ATTRIBUTE_HIDDEN != 0 || attrs & FILE_ATTRIBUTE_SYSTEM != 0;
        }
    }
    false
}

#[cfg(unix)]
fn permissions_of(meta: &std::fs::Metadata) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode();
    Some(crate::remote_path::mode_to_string(mode, meta.is_dir()))
}

#[cfg(not(unix))]
fn permissions_of(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

async fn entry_from_path(path: PathBuf) -> Option<LocalEntry> {
    let name = file_name(&path);
    let symlink_meta = tokio::fs::symlink_metadata(&path).await.ok()?;
    let is_symlink = symlink_meta.file_type().is_symlink();
    // Follow symlinks for size/kind, but keep the link flag.
    let meta = if is_symlink {
        tokio::fs::metadata(&path)
            .await
            .unwrap_or_else(|_| symlink_meta.clone())
    } else {
        symlink_meta.clone()
    };
    let is_dir = meta.is_dir();
    let kind = if is_dir {
        LocalKind::Directory
    } else {
        LocalKind::File
    };
    let hidden = is_hidden(&path, &name);
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty());
    Some(LocalEntry {
        name,
        path: path.to_string_lossy().to_string(),
        kind,
        is_dir,
        is_symlink,
        size: if is_dir { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(rfc3339),
        hidden,
        extension,
        permissions: permissions_of(&meta),
        free_space: None,
    })
}

fn place(name: &str, path: Option<PathBuf>) -> Option<LocalEntry> {
    let path = path?;
    if !path.is_dir() {
        return None;
    }
    Some(LocalEntry {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        kind: LocalKind::Place,
        is_dir: true,
        is_symlink: false,
        size: 0,
        modified: None,
        hidden: false,
        extension: None,
        permissions: None,
        free_space: None,
    })
}

/// Known folders shown in the sidebar / "This PC" view.
pub fn places() -> Vec<LocalEntry> {
    let mut result = Vec::new();
    if let Some(home) = dirs_home() {
        result.push(place("Home", Some(home.clone())));
        result.push(place("Desktop", Some(home.join("Desktop"))));
        result.push(place("Downloads", Some(home.join("Downloads"))));
        result.push(place("Documents", Some(home.join("Documents"))));
        result.push(place("Pictures", Some(home.join("Pictures"))));
        result.push(place("Videos", Some(home.join("Videos"))));
    }
    result.into_iter().flatten().collect()
}

fn dirs_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Expand a leading `~` (as stored by older FileZilla sites) to the home folder.
pub fn expand_local_path(path: &str) -> PathBuf {
    let path = path.trim();
    let rest = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("~\\"))
        .or_else(|| (path == "~").then_some(""));
    match (rest, dirs_home()) {
        (Some(rest), Some(home)) => {
            if rest.is_empty() {
                home
            } else {
                home.join(rest)
            }
        }
        _ => PathBuf::from(path),
    }
}

async fn drives() -> Vec<LocalEntry> {
    let mut result = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            let path = PathBuf::from(&root);
            if tokio::fs::metadata(&path).await.is_ok() {
                result.push(LocalEntry {
                    name: format!("{}:", letter as char),
                    path: root,
                    kind: LocalKind::Drive,
                    is_dir: true,
                    is_symlink: false,
                    size: 0,
                    modified: None,
                    hidden: false,
                    extension: None,
                    permissions: None,
                    free_space: None,
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        result.push(LocalEntry {
            name: "/".to_string(),
            path: "/".to_string(),
            kind: LocalKind::Drive,
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: None,
            hidden: false,
            extension: None,
            permissions: None,
            free_space: None,
        });
    }
    result
}

pub async fn list(path: Option<&str>, show_hidden: bool) -> AppResult<LocalListing> {
    let Some(raw) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        // Virtual "This PC" root.
        let mut entries = drives().await;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        return Ok(LocalListing {
            path: None,
            parent: None,
            entries,
            places: places(),
        });
    };

    let dir = expand_local_path(raw);
    let meta = tokio::fs::metadata(&dir)
        .await
        .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot open {raw}: {e}")))?;
    if !meta.is_dir() {
        return Err(AppError::invalid(format!("{raw} is not a directory")));
    }

    let mut reader = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot list {raw}: {e}")))?;
    let mut entries: Vec<LocalEntry> = Vec::new();
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot read {raw}: {e}")))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        if !show_hidden && is_hidden(&entry.path(), &name) {
            continue;
        }
        if let Some(item) = entry_from_path(entry.path()).await {
            entries.push(item);
        }
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let parent = dir.parent().and_then(|p| {
        if p.as_os_str().is_empty() {
            None
        } else {
            Some(p.to_string_lossy().to_string())
        }
    });

    Ok(LocalListing {
        path: Some(dir.to_string_lossy().to_string()),
        parent,
        entries,
        places: places(),
    })
}

pub async fn create_dir(path: &str) -> AppResult<()> {
    let path = expand_local_path(path);
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot create folder: {e}")))
}

pub async fn rename(from: &str, to: &str) -> AppResult<()> {
    let from = expand_local_path(from);
    let to = expand_local_path(to);
    if to.exists() {
        return Err(AppError::invalid("A file with that name already exists"));
    }
    tokio::fs::rename(&from, &to)
        .await
        .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot rename: {e}")))
}

pub async fn delete(path: &str) -> AppResult<()> {
    let target = expand_local_path(path);
    let meta = tokio::fs::symlink_metadata(&target)
        .await
        .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot access {path}: {e}")))?;
    if meta.is_dir() {
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot delete folder: {e}")))
    } else {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| AppError::new("LOCAL_IO", format!("Cannot delete file: {e}")))
    }
}

#[cfg(test)]
pub fn default_start_dir() -> String {
    dirs_home()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "C:\\".to_string()
            } else {
                "/".to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lists_this_pc() {
        let listing = list(None, true).await.unwrap();
        assert!(listing.path.is_none());
        assert!(!listing.entries.is_empty());
    }

    #[tokio::test]
    async fn lists_home() {
        let home = default_start_dir();
        let listing = list(Some(&home), true).await.unwrap();
        assert_eq!(listing.path.as_deref(), Some(home.as_str()));
    }
}
