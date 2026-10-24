//! Persistence for sites, groups, settings and trusted SSH host keys.
//!
//! The on-disk format is intentionally boring JSON so users can inspect it and
//! the frontend contract stays stable. Writes are atomic (temp file + rename).

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, KnownHost, Site, SiteGroup, SiteInput, SitesSnapshot};
use crate::secrets;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use uuid::Uuid;

const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SitesFile {
    #[serde(default = "store_version")]
    version: u32,
    #[serde(default)]
    sites: Vec<Site>,
    #[serde(default)]
    groups: Vec<SiteGroup>,
}

fn store_version() -> u32 {
    STORE_VERSION
}

impl Default for SitesFile {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            sites: Vec::new(),
            groups: Vec::new(),
        }
    }
}

pub struct Store {
    dir: PathBuf,
    data: RwLock<SitesFile>,
    settings: RwLock<AppSettings>,
    known_hosts: Mutex<HashMap<String, KnownHost>>,
    kiosk_mode: u8,
    read_only: bool,
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::new("IO", format!("Cannot create {}: {e}", parent.display())))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, bytes)
        .map_err(|e| AppError::new("IO", format!("Cannot write {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| AppError::new("IO", format!("Cannot replace {}: {e}", path.display())))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

impl Store {
    #[allow(dead_code)]
    pub fn load(dir: PathBuf) -> Self {
        Self::load_with_policy(dir, 0, false)
    }

    pub fn load_with_policy(dir: PathBuf, kiosk_mode: u8, read_only: bool) -> Self {
        let data: SitesFile = read_json(&dir.join("sites.json"));
        let settings: AppSettings = read_json(&dir.join("settings.json"));
        let known: Vec<KnownHost> = read_json(&dir.join("known_hosts.json"));
        let known_hosts = known
            .into_iter()
            .map(|k| (format!("{}|{}", k.host, k.key_type), k))
            .collect();
        Self {
            dir,
            data: RwLock::new(data),
            settings: RwLock::new(settings),
            known_hosts: Mutex::new(known_hosts),
            kiosk_mode,
            read_only,
        }
    }

    pub fn kiosk_mode(&self) -> u8 {
        self.kiosk_mode
    }

    fn writable(&self) -> AppResult<()> {
        if self.read_only {
            Err(AppError::new(
                "READ_ONLY",
                "This installation is managed and its configuration is read-only",
            ))
        } else {
            Ok(())
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Write an auxiliary JSON file (queue, UI state) under the config dir.
    pub fn write_aux(&self, name: &str, bytes: &[u8]) -> AppResult<()> {
        if self.read_only {
            return Ok(());
        }
        atomic_write(&self.dir.join(name), bytes)
    }

    fn save_sites(&self) -> AppResult<()> {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        let bytes = serde_json::to_vec_pretty(&*data)?;
        atomic_write(&self.dir.join("sites.json"), &bytes)
    }

    pub fn snapshot(&self) -> SitesSnapshot {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        let mut sites = data.sites.clone();
        sites.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        SitesSnapshot {
            sites,
            groups: data.groups.clone(),
        }
    }

    pub fn site(&self, id: &str) -> Option<Site> {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        data.sites.iter().find(|s| s.id == id).cloned()
    }

    pub fn save_site(&self, mut input: SiteInput) -> AppResult<Site> {
        self.writable()?;
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::invalid("Site name is required"));
        }
        input.config.validate()?;

        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let existing = input
            .id
            .as_deref()
            .and_then(|id| data.sites.iter().position(|s| s.id == id));
        let id = existing
            .map(|idx| data.sites[idx].id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = existing
            .map(|idx| data.sites[idx].created_at)
            .unwrap_or_else(now_ms);
        let mut has_password = existing
            .map(|idx| data.sites[idx].has_password)
            .unwrap_or(false);

        if self.kiosk_mode == 0 {
            if input.clear_password {
                secrets::delete_all(&id).ok();
                has_password = false;
            }
            if let Some(password) = input.password.as_deref() {
                if !password.is_empty() {
                    secrets::set_password(&id, password)?;
                    has_password = true;
                }
            }
            if let Some(passphrase) = input.key_passphrase.as_deref() {
                if !passphrase.is_empty() {
                    secrets::set_key_passphrase(&id, passphrase)?;
                }
            }
            if let Some(proxy) = input.proxy_password.as_deref() {
                if !proxy.is_empty() {
                    secrets::set_proxy_password(&id, proxy)?;
                }
            }
        } else if existing.is_none() {
            has_password = false;
        }

        let site = Site {
            id: id.clone(),
            name,
            group_id: input.group_id.filter(|g| !g.is_empty()),
            config: input.config,
            has_password,
            created_at,
            updated_at: now_ms(),
        };
        match existing {
            Some(idx) => data.sites[idx] = site.clone(),
            None => data.sites.push(site.clone()),
        }
        drop(data);
        self.save_sites()?;
        Ok(site)
    }

    pub fn delete_site(&self, id: &str) -> AppResult<()> {
        self.writable()?;
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let before = data.sites.len();
        data.sites.retain(|s| s.id != id);
        if data.sites.len() == before {
            return Err(AppError::not_found("Site not found"));
        }
        drop(data);
        secrets::delete_all(id).ok();
        self.save_sites()
    }

    pub fn duplicate_site(&self, id: &str) -> AppResult<Site> {
        self.writable()?;
        let site = self
            .site(id)
            .ok_or_else(|| AppError::not_found("Site not found"))?;
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let new_id = Uuid::new_v4().to_string();
        let copy = Site {
            id: new_id.clone(),
            name: format!("{} (copy)", site.name),
            group_id: site.group_id.clone(),
            config: site.config.clone(),
            has_password: false,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        data.sites.push(copy.clone());
        drop(data);
        // Deliberately do not copy secrets: the duplicate starts without one.
        self.save_sites()?;
        Ok(copy)
    }

    pub fn create_group(&self, name: &str, parent_id: Option<String>) -> AppResult<SiteGroup> {
        self.writable()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::invalid("Group name is required"));
        }
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        if let Some(parent) = parent_id.as_deref() {
            if !data.groups.iter().any(|g| g.id == parent) {
                return Err(AppError::not_found("Parent group not found"));
            }
        }
        let group = SiteGroup {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            parent_id,
        };
        data.groups.push(group.clone());
        drop(data);
        self.save_sites()?;
        Ok(group)
    }

    pub fn rename_group(&self, id: &str, name: &str) -> AppResult<SiteGroup> {
        self.writable()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::invalid("Group name is required"));
        }
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let group = data
            .groups
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| AppError::not_found("Group not found"))?;
        group.name = name.to_string();
        let result = group.clone();
        drop(data);
        self.save_sites()?;
        Ok(result)
    }

    pub fn delete_group(&self, id: &str) -> AppResult<()> {
        self.writable()?;
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let parent = data
            .groups
            .iter()
            .find(|g| g.id == id)
            .and_then(|g| g.parent_id.clone());
        let mut remove: Vec<String> = Vec::new();
        fn collect(groups: &[SiteGroup], id: &str, out: &mut Vec<String>) {
            out.push(id.to_string());
            for child in groups.iter().filter(|g| g.parent_id.as_deref() == Some(id)) {
                collect(groups, &child.id, out);
            }
        }
        collect(&data.groups, id, &mut remove);
        data.groups.retain(|g| !remove.contains(&g.id));
        for site in data.sites.iter_mut() {
            if site
                .group_id
                .as_deref()
                .is_some_and(|g| remove.iter().any(|r| r == g))
            {
                site.group_id = parent.clone();
            }
        }
        drop(data);
        self.save_sites()
    }

    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn save_settings(&self, mut settings: AppSettings) -> AppResult<AppSettings> {
        self.writable()?;
        settings.concurrent_transfers = settings.concurrent_transfers.clamp(1, 10);
        settings.theme = match settings.theme.as_str() {
            "light" | "dark" => settings.theme,
            _ => "system".into(),
        };
        let language = settings.language.trim();
        let accent = settings.accent.clone();
        settings.accent = match accent.as_str() {
            "teal" | "purple" | "orange" | "pink" | "green" => accent,
            _ => "blue".into(),
        };
        settings.language = if language.len() >= 2
            && language.len() <= 32
            && language
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '@'))
        {
            language.to_string()
        } else {
            "zh-CN".into()
        };
        settings.density = match settings.density.as_str() {
            "compact" => "compact".into(),
            _ => "comfortable".into(),
        };
        settings.double_click_action = match settings.double_click_action.as_str() {
            "open" => "open".into(),
            _ => "transfer".into(),
        };
        let bytes = serde_json::to_vec_pretty(&settings)?;
        atomic_write(&self.dir.join("settings.json"), &bytes)?;
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = settings.clone();
        Ok(settings)
    }

    pub fn known_host(&self, host: &str, port: u16, key_type: &str) -> Option<String> {
        let map = self.known_hosts.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&format!("{host}:{port}|{key_type}"))
            .map(|k| k.fingerprint.clone())
    }

    pub fn trust_known_host(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) {
        let mut map = self.known_hosts.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(
            format!("{host}:{port}|{key_type}"),
            KnownHost {
                host: format!("{host}:{port}"),
                key_type: key_type.to_string(),
                fingerprint: fingerprint.to_string(),
                added_at: now_ms(),
            },
        );
        let list: Vec<KnownHost> = map.values().cloned().collect();
        drop(map);
        if !self.read_only {
            if let Ok(bytes) = serde_json::to_vec_pretty(&list) {
                let _ = atomic_write(&self.dir.join("known_hosts.json"), &bytes);
            }
        }
    }

    pub fn known_hosts(&self) -> Vec<KnownHost> {
        let map = self.known_hosts.lock().unwrap_or_else(|e| e.into_inner());
        let mut list: Vec<KnownHost> = map.values().cloned().collect();
        list.sort_by(|a, b| a.host.cmp(&b.host));
        list
    }

    pub fn forget_host(&self, host: &str) -> AppResult<()> {
        self.writable()?;
        let mut map = self.known_hosts.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, v| v.host != host);
        let list: Vec<KnownHost> = map.values().cloned().collect();
        drop(map);
        let bytes = serde_json::to_vec_pretty(&list)?;
        atomic_write(&self.dir.join("known_hosts.json"), &bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthMethod, Protocol, SiteConfig, SiteInput, TransferMode};

    fn site_input(password: Option<&str>) -> SiteInput {
        SiteInput {
            id: None,
            name: "kiosk test".into(),
            group_id: None,
            config: SiteConfig {
                protocol: Protocol::Ftp,
                host: "example.test".into(),
                port: 21,
                username: "user".into(),
                auth_method: AuthMethod::Password,
                private_key_path: None,
                initial_remote_dir: None,
                local_dir: None,
                passive_mode: true,
                verify_certificate: true,
                connect_timeout_secs: 20,
                keepalive_secs: 0,
                transfer_mode: TransferMode::Auto,
                encoding: "UTF-8".into(),
                max_connections: 1,
                notes: None,
                proxy_type: String::new(),
                proxy_host: String::new(),
                proxy_port: 0,
                proxy_username: String::new(),
            },
            password: password.map(str::to_string),
            key_passphrase: None,
            proxy_password: None,
            clear_password: false,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-store")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_only_rejects_site_writes() {
        let store = Store::load_with_policy(temp_dir("read-only"), 0, true);
        let error = store.save_site(site_input(None)).unwrap_err();
        assert_eq!(error.code, "READ_ONLY");
        assert!(store.snapshot().sites.is_empty());
    }

    #[test]
    fn kiosk_mode_does_not_store_passwords() {
        let store = Store::load_with_policy(temp_dir("kiosk"), 1, false);
        let site = store.save_site(site_input(Some("secret"))).unwrap();
        assert!(!site.has_password);
        assert!(store.snapshot().sites[0].has_password == false);
    }
}
