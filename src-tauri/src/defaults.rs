//! Enterprise defaults from FileZilla's `fzdefaults.xml`.
//!
//! Supported settings:
//! - `Config Location` (absolute, relative, `.` or `$ENV` paths)
//! - `Kiosk mode` (0 = normal, 1 = do not store passwords, 2 = read-only config)
//! - `Disable update check`
//! - `Cache directory` (recorded; this client currently keeps no disk cache)
//!
//! `<Servers>` default entries are parsed by the site-import code and can be
//! seeded into the Site Manager on first run.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPolicy {
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_dir: Option<String>,
    pub kiosk_mode: u8,
    pub read_only: bool,
    pub disable_update_check: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl ManagedPolicy {
    pub fn no_store_passwords(&self) -> bool {
        self.kiosk_mode >= 1
    }
}

#[derive(Debug, Default, Deserialize)]
struct Root {
    #[serde(rename = "Settings", default)]
    settings: SettingsNode,
    #[serde(rename = "Servers", default)]
    _servers: Option<ServersNode>,
}

#[derive(Debug, Default, Deserialize)]
struct SettingsNode {
    #[serde(rename = "Setting", default)]
    items: Vec<SettingNode>,
}

#[derive(Debug, Deserialize)]
struct SettingNode {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "$text", default)]
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct ServersNode {
    #[serde(rename = "Server", default)]
    _servers: Vec<ServerNode>,
    #[serde(rename = "Folder", default)]
    _folders: Vec<FolderNode>,
}

#[derive(Debug, Default, Deserialize)]
struct ServerNode {}

#[derive(Debug, Default, Deserialize)]
struct FolderNode {}

pub fn load(exe_dir: Option<&Path>, app_config: Option<&Path>) -> ManagedPolicy {
    locate(exe_dir, app_config)
        .map(|path| load_from(&path))
        .unwrap_or_default()
}

pub fn load_from(path: &Path) -> ManagedPolicy {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            return ManagedPolicy {
                managed: true,
                loaded_from: Some(path.to_string_lossy().to_string()),
                warning: Some(format!("Cannot read fzdefaults.xml: {err}")),
                ..ManagedPolicy::default()
            }
        }
    };
    match quick_xml::de::from_str::<Root>(&source) {
        Ok(root) => apply(root, path),
        Err(err) => ManagedPolicy {
            managed: true,
            loaded_from: Some(path.to_string_lossy().to_string()),
            warning: Some(format!("Cannot parse fzdefaults.xml: {err}")),
            ..ManagedPolicy::default()
        },
    }
}

fn apply(root: Root, path: &Path) -> ManagedPolicy {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut policy = ManagedPolicy {
        managed: true,
        loaded_from: Some(path.to_string_lossy().to_string()),
        ..ManagedPolicy::default()
    };
    for setting in root.settings.items {
        let value = setting.value.trim();
        match setting.name.trim() {
            "Config Location" if !value.is_empty() => {
                policy.config_dir = Some(expand_path(value, base).to_string_lossy().to_string());
            }
            "Kiosk mode" => {
                policy.kiosk_mode = value.parse::<u8>().unwrap_or(0).min(2);
                policy.read_only = policy.kiosk_mode >= 2;
            }
            "Disable update check" => {
                policy.disable_update_check = matches!(value, "1" | "true" | "yes" | "on");
            }
            "Cache directory" if !value.is_empty() => {
                policy.cache_dir = Some(expand_path(value, base).to_string_lossy().to_string());
            }
            _ => {}
        }
    }
    policy
}

fn locate(exe_dir: Option<&Path>, app_config: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("FILEZILLA_RS_FZDEFAULTS") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut candidates = Vec::new();
    if let Some(dir) = exe_dir {
        candidates.push(dir.join("fzdefaults.xml"));
    }
    if let Some(dir) = app_config {
        candidates.push(dir.join("fzdefaults.xml"));
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join("FileZilla").join("fzdefaults.xml"));
        }
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".filezilla").join("fzdefaults.xml"));
    }
    #[cfg(not(windows))]
    candidates.push(PathBuf::from("/etc/filezilla/fzdefaults.xml"));

    candidates.into_iter().find(|path| path.is_file())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Expand `$ENV`, `$$`, `.` and relative paths the same way FileZilla does.
pub fn expand_path(value: &str, base: &Path) -> PathBuf {
    let mut expanded = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            expanded.push(ch);
            continue;
        }
        if chars.peek() == Some(&'$') {
            chars.next();
            expanded.push('$');
            continue;
        }
        let mut name = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if name.is_empty() {
            expanded.push('$');
        } else if let Ok(value) = std::env::var(&name) {
            expanded.push_str(&value);
        } else {
            expanded.push('$');
            expanded.push_str(&name);
        }
    }
    let path = PathBuf::from(expanded);
    let path = if path == Path::new(".") {
        base.to_path_buf()
    } else if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    let mut clean = PathBuf::new();
    for component in path.components() {
        if component != Component::CurDir {
            clean.push(component.as_os_str());
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_managed_policy() {
        let xml = r#"<FileZilla3>
          <Settings>
            <Setting name="Config Location">./managed-config</Setting>
            <Setting name="Kiosk mode">1</Setting>
            <Setting name="Disable update check">1</Setting>
          </Settings>
        </FileZilla3>"#;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fzdefaults-test.xml");
        std::fs::write(&path, xml).unwrap();
        let policy = load_from(&path);
        std::fs::remove_file(&path).ok();
        assert!(policy.managed);
        assert_eq!(policy.kiosk_mode, 1);
        assert!(!policy.read_only);
        assert!(policy.disable_update_check);
        assert!(policy
            .config_dir
            .as_deref()
            .unwrap_or_default()
            .ends_with("managed-config"));
    }

    #[test]
    fn expands_dollar_and_relative_paths() {
        let base = Path::new("C:\\ProgramData\\FileZilla");
        assert_eq!(expand_path(".", base), base);
        assert_eq!(expand_path("profile", base), base.join("profile"));
        assert_eq!(expand_path("$$cache", base), base.join("$cache"));
        assert_eq!(expand_path("C:\\data", base), PathBuf::from("C:\\data"));
    }
}
