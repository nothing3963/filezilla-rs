//! FileZilla Site Manager XML import/export.
//!
//! This reads/writes the `sitemanager.xml` dialect used by the original
//! client, including nested `<Folder>` groups.  Passwords are stored with
//! FileZilla's base64 obfuscation in the XML; import moves them into the OS
//! credential vault.

use crate::error::{AppError, AppResult};
use crate::models::{AuthMethod, Protocol, SiteConfig, SiteInput, TransferMode};
use crate::secrets;
use crate::store::Store;
use base64::Engine;
use quick_xml::escape::escape;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub imported: usize,
    pub skipped: usize,
    pub groups_created: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoImportOutcome {
    pub sources: Vec<String>,
    pub imported: usize,
    pub skipped: usize,
    pub groups_created: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSites {
    pub source: String,
}

#[derive(Debug, Default, Deserialize)]
struct Root {
    #[serde(rename = "Servers", default)]
    servers: Servers,
    #[serde(rename = "RecentServers", default)]
    recent_servers: Servers,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct Servers {
    #[serde(rename = "Folder", default)]
    folders: Vec<Folder>,
    #[serde(rename = "Server", default)]
    servers: Vec<ServerXml>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct Folder {
    #[serde(rename = "@name", default)]
    name: String,
    #[serde(rename = "Folder", default)]
    folders: Vec<Folder>,
    #[serde(rename = "Server", default)]
    servers: Vec<ServerXml>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct ServerXml {
    #[serde(rename = "Host", default)]
    host: Option<String>,
    #[serde(rename = "Name", default)]
    name: Option<String>,
    #[serde(rename = "CustomEncoding", default)]
    custom_encoding: Option<String>,
    #[serde(rename = "Port", default)]
    port: Option<u16>,
    #[serde(rename = "Protocol", default)]
    protocol: Option<u8>,
    #[serde(rename = "Logontype", default)]
    logon_type: Option<u8>,
    #[serde(rename = "User", default)]
    user: Option<String>,
    #[serde(rename = "Pass", default)]
    pass: Option<String>,
    #[serde(rename = "Keyfile", default)]
    key_file: Option<String>,
    #[serde(rename = "PasvMode", default)]
    pasv_mode: Option<String>,
    #[serde(rename = "EncodingType", default)]
    encoding: Option<String>,
    #[serde(rename = "MaximumMultipleConnections", default)]
    max_connections: Option<u8>,
    #[serde(rename = "Comments", default)]
    comments: Option<String>,
    #[serde(rename = "LocalDir", default)]
    local_dir: Option<String>,
    #[serde(rename = "RemoteDir", default)]
    remote_dir: Option<String>,
}

struct PendingSite {
    name: String,
    group_path: Vec<String>,
    config: SiteConfig,
    password: Option<String>,
}

pub fn import_file(store: &Store, path: &Path) -> AppResult<ImportOutcome> {
    let xml = std::fs::read_to_string(path)
        .map_err(|e| AppError::new("IO", format!("Cannot read {}: {e}", path.display())))?;
    import_xml(store, &xml)
}

/// Imports FileZilla's own site list without asking for a path.
///
/// Looks in the managed `Config Location` (when `fzdefaults.xml` defines one)
/// and the standard per-user configuration directories. `sitemanager.xml` is
/// preferred; `recentservers.xml` is only used when no saved site list exists.
/// Returns `None` when FileZilla has never been configured on this machine.
pub fn import_detected(
    store: &Store,
    managed_config_dir: Option<&str>,
) -> AppResult<Option<AutoImportOutcome>> {
    let Some(path) = detect(managed_config_dir) else {
        return Ok(None);
    };

    let outcome = import_file(store, &path)?;
    Ok(Some(AutoImportOutcome {
        sources: vec![path.display().to_string()],
        imported: outcome.imported,
        skipped: outcome.skipped,
        groups_created: outcome.groups_created,
    }))
}

/// Finds FileZilla's site list without importing it.
pub fn detect(managed_config_dir: Option<&str>) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = managed_config_dir.map(PathBuf::from) {
        dirs.push(dir);
    }
    for dir in filezilla_config_dirs() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }

    ["sitemanager.xml", "recentservers.xml"]
        .into_iter()
        .find_map(|name| dirs.iter().map(|dir| dir.join(name)).find(|path| path.is_file()))
}

fn filezilla_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("FileZilla"));
    }
    #[cfg(not(windows))]
    {
        if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
            dirs.push(PathBuf::from(config).join("filezilla"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(&home).join(".config").join("filezilla"));
        }
        #[cfg(target_os = "macos")]
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("FileZilla"),
            );
        }
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        dirs.push(PathBuf::from(home).join(".filezilla"));
    }
    dirs
}

pub fn import_xml(store: &Store, xml: &str) -> AppResult<ImportOutcome> {
    let root: Root = quick_xml::de::from_str(xml)
        .map_err(|e| AppError::new("INVALID_INPUT", format!("Invalid Site Manager XML: {e}")))?;
    let mut pending = Vec::new();
    collect(&root.servers, &[], &mut pending);
    collect(&root.recent_servers, &[], &mut pending);
    if pending.is_empty() {
        return Err(AppError::invalid("The XML file contains no sites"));
    }

    let mut existing_names: Vec<String> = store
        .snapshot()
        .sites
        .into_iter()
        .map(|site| site.name)
        .collect();
    let mut groups_created = 0;
    let mut imported = 0;
    let mut skipped = 0;

    for mut site in pending {
        site.config.validate().ok();
        if site.config.host.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let group_id = ensure_group(store, &site.group_path, &mut groups_created)?;
        let name = unique_name(&site.name, &existing_names);
        let input = SiteInput {
            id: None,
            name,
            group_id,
            config: site.config,
            password: site.password,
            key_passphrase: None,
            proxy_password: None,
            clear_password: false,
        };
        match store.save_site(input) {
            Ok(saved) => {
                existing_names.push(saved.name);
                imported += 1;
            }
            Err(err) => {
                tracing::warn!("import site failed: {}", err.message);
                skipped += 1;
            }
        }
    }

    Ok(ImportOutcome {
        imported,
        skipped,
        groups_created,
    })
}

fn collect(servers: &Servers, path: &[String], out: &mut Vec<PendingSite>) {
    for server in &servers.servers {
        if let Some(site) = convert(server, path) {
            out.push(site);
        }
    }
    for folder in &servers.folders {
        let mut next = path.to_vec();
        if !folder.name.trim().is_empty() {
            next.push(folder.name.trim().to_string());
        }
        let child = Servers {
            folders: folder.folders.clone(),
            servers: folder.servers.clone(),
        };
        collect(&child, &next, out);
    }
}

fn convert(server: &ServerXml, group_path: &[String]) -> Option<PendingSite> {
    let host = server.host.as_deref().unwrap_or_default().trim();
    if host.is_empty() {
        return None;
    }
    let protocol = match server.protocol.unwrap_or(0) {
        1 => Protocol::Sftp,
        3 => Protocol::FtpsImplicit,
        4 => Protocol::Ftps,
        _ => Protocol::Ftp,
    };
    let user = server.user.clone().unwrap_or_default();
    let key_file = server
        .key_file
        .clone()
        .filter(|value| !value.trim().is_empty());
    let auth_method = if key_file.is_some() || server.logon_type == Some(5) {
        AuthMethod::Key
    } else {
        AuthMethod::Password
    };
    let password = server
        .pass
        .as_deref()
        .filter(|value| !value.is_empty())
        .and_then(decode_password);
    let name = server
        .name
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if user.is_empty() && protocol != Protocol::Sftp {
                format!("{host} (anonymous)")
            } else {
                host.to_string()
            }
        });
    let port = server.port.unwrap_or(0);
    let config = SiteConfig {
        protocol,
        host: host.to_string(),
        port: if port == 0 {
            protocol.default_port()
        } else {
            port
        },
        username: user,
        auth_method,
        private_key_path: key_file,
        initial_remote_dir: server
            .remote_dir
            .clone()
            .filter(|value| !value.trim().is_empty()),
        local_dir: server
            .local_dir
            .clone()
            .filter(|value| !value.trim().is_empty()),
        passive_mode: !matches!(server.pasv_mode.as_deref(), Some("MODE_ACTIVE") | Some("1")),
        verify_certificate: true,
        connect_timeout_secs: 20,
        keepalive_secs: 30,
        transfer_mode: TransferMode::Auto,
        encoding: normalize_encoding(
            server.encoding.as_deref(),
            server.custom_encoding.as_deref(),
        ),
        max_connections: server.max_connections.unwrap_or(2).clamp(1, 10),
        notes: server
            .comments
            .clone()
            .filter(|value| !value.trim().is_empty()),
        proxy_type: String::new(),
        proxy_host: String::new(),
        proxy_port: 0,
        proxy_username: String::new(),
    };
    Some(PendingSite {
        name,
        group_path: group_path.to_vec(),
        config,
        password,
    })
}

fn decode_password(value: &str) -> Option<String> {
    base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn normalize_encoding(value: Option<&str>, custom: Option<&str>) -> String {
    match value.unwrap_or("Auto").trim() {
        "" | "Auto" | "auto" => "UTF-8".to_string(),
        "Custom" | "custom" => custom
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("UTF-8")
            .to_string(),
        other => other.to_string(),
    }
}

fn unique_name(name: &str, existing: &[String]) -> String {
    if !existing.iter().any(|item| item == name) {
        return name.to_string();
    }
    for index in 2..1000 {
        let candidate = format!("{name} ({index})");
        if !existing.iter().any(|item| item == &candidate) {
            return candidate;
        }
    }
    format!("{name} (imported)")
}

fn ensure_group(store: &Store, path: &[String], created: &mut usize) -> AppResult<Option<String>> {
    let mut parent: Option<String> = None;
    for name in path {
        let snapshot = store.snapshot();
        let existing = snapshot
            .groups
            .into_iter()
            .find(|group| group.name.eq_ignore_ascii_case(name) && group.parent_id == parent);
        parent = match existing {
            Some(group) => Some(group.id),
            None => {
                let group = store.create_group(name, parent.clone())?;
                *created += 1;
                Some(group.id)
            }
        };
    }
    Ok(parent)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    pub sites: usize,
    pub groups: usize,
    pub passwords_included: bool,
}

pub fn export_file(
    store: &Store,
    path: &Path,
    include_passwords: bool,
) -> AppResult<ExportOutcome> {
    let snapshot = store.snapshot();
    let mut xml =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<FileZilla3>\n  <Servers>\n");
    let roots: Vec<_> = snapshot
        .groups
        .iter()
        .filter(|group| group.parent_id.is_none())
        .collect();
    let mut groups_written = 0;
    for group in roots {
        write_folder(
            &mut xml,
            store,
            &snapshot,
            group,
            2,
            include_passwords,
            &mut groups_written,
        );
    }
    for site in snapshot.sites.iter().filter(|site| site.group_id.is_none()) {
        write_site(&mut xml, store, site, 2, include_passwords);
    }
    xml.push_str("  </Servers>\n</FileZilla3>\n");
    std::fs::write(path, xml)
        .map_err(|e| AppError::new("IO", format!("Cannot write {}: {e}", path.display())))?;
    Ok(ExportOutcome {
        sites: snapshot.sites.len(),
        groups: snapshot.groups.len(),
        passwords_included: include_passwords,
    })
}

fn write_folder(
    xml: &mut String,
    store: &Store,
    snapshot: &crate::models::SitesSnapshot,
    group: &crate::models::SiteGroup,
    indent: usize,
    include_passwords: bool,
    groups_written: &mut usize,
) {
    let pad = "  ".repeat(indent);
    xml.push_str(&format!("{pad}<Folder name=\"{}\">\n", escape(&group.name)));
    *groups_written += 1;
    for child in snapshot
        .groups
        .iter()
        .filter(|item| item.parent_id.as_deref() == Some(group.id.as_str()))
    {
        write_folder(
            xml,
            store,
            snapshot,
            child,
            indent + 1,
            include_passwords,
            groups_written,
        );
    }
    for site in snapshot
        .sites
        .iter()
        .filter(|site| site.group_id.as_deref() == Some(group.id.as_str()))
    {
        write_site(xml, store, site, indent + 1, include_passwords);
    }
    xml.push_str(&format!("{pad}</Folder>\n"));
}

fn write_site(
    xml: &mut String,
    store: &Store,
    site: &crate::models::Site,
    indent: usize,
    include_passwords: bool,
) {
    let pad = "  ".repeat(indent);
    let cfg = &site.config;
    let protocol = match cfg.protocol {
        Protocol::Ftp => 6,
        Protocol::Sftp => 1,
        Protocol::FtpsImplicit => 3,
        Protocol::Ftps => 4,
    };
    let logon = if cfg.auth_method == AuthMethod::Key {
        5
    } else if cfg.username.trim().is_empty() {
        0
    } else {
        1
    };
    let pass = if include_passwords {
        store
            .site(&site.id)
            .and_then(|_| secrets::get_password(&site.id))
            .map(|password| base64::engine::general_purpose::STANDARD.encode(password.as_bytes()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    xml.push_str(&format!("{pad}<Server>\n"));
    xml.push_str(&format!("{pad}  <Host>{}</Host>\n", escape(&cfg.host)));
    xml.push_str(&format!("{pad}  <Name>{}</Name>\n", escape(&site.name)));
    xml.push_str(&format!("{pad}  <Port>{}</Port>\n", cfg.effective_port()));
    xml.push_str(&format!("{pad}  <Protocol>{protocol}</Protocol>\n"));
    xml.push_str(&format!("{pad}  <Type>0</Type>\n"));
    xml.push_str(&format!("{pad}  <Logontype>{logon}</Logontype>\n"));
    xml.push_str(&format!("{pad}  <User>{}</User>\n", escape(&cfg.username)));
    xml.push_str(&format!("{pad}  <Pass encoding=\"base64\">{pass}</Pass>\n"));
    if let Some(key) = cfg.private_key_path.as_deref() {
        xml.push_str(&format!("{pad}  <Keyfile>{}</Keyfile>\n", escape(key)));
    }
    xml.push_str(&format!(
        "{pad}  <PasvMode>{}</PasvMode>\n",
        if cfg.passive_mode {
            "MODE_PASSIVE"
        } else {
            "MODE_ACTIVE"
        }
    ));
    xml.push_str(&format!(
        "{pad}  <MaximumMultipleConnections>{}</MaximumMultipleConnections>\n",
        cfg.max_connections
    ));
    let standard_encoding = matches!(
        cfg.encoding.to_ascii_uppercase().as_str(),
        "UTF-8" | "AUTO" | "ISO-8859-1" | "ISO-8859-15" | "WINDOWS-1252"
    );
    if standard_encoding {
        xml.push_str(&format!(
            "{pad}  <EncodingType>{}</EncodingType>\n",
            escape(&cfg.encoding)
        ));
    } else {
        xml.push_str(&format!("{pad}  <EncodingType>Custom</EncodingType>\n"));
        xml.push_str(&format!(
            "{pad}  <CustomEncoding>{}</CustomEncoding>\n",
            escape(&cfg.encoding)
        ));
    }
    xml.push_str(&format!(
        "{pad}  <Comments>{}</Comments>\n",
        escape(cfg.notes.as_deref().unwrap_or_default())
    ));
    xml.push_str(&format!(
        "{pad}  <LocalDir>{}</LocalDir>\n",
        escape(cfg.local_dir.as_deref().unwrap_or_default())
    ));
    xml.push_str(&format!(
        "{pad}  <RemoteDir>{}</RemoteDir>\n",
        escape(cfg.initial_remote_dir.as_deref().unwrap_or_default())
    ));
    xml.push_str(&format!("{pad}</Server>\n"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    const SAMPLE: &str = r#"<FileZilla3>
      <Servers>
        <Folder name="Work">
          <Server>
            <Host>test.rebex.net</Host>
            <Name>Rebex Work</Name>
            <Port>21</Port>
            <Protocol>4</Protocol>
            <Logontype>1</Logontype>
            <User>demo</User>
            <Pass encoding="base64">cGFzc3dvcmQ=</Pass>
            <PasvMode>MODE_PASSIVE</PasvMode>
            <EncodingType>Custom</EncodingType>
            <CustomEncoding>GBK</CustomEncoding>
            <RemoteDir>/pub</RemoteDir>
          </Server>
        </Folder>
      </Servers>
    </FileZilla3>"#;

    #[test]
    fn imports_recentservers_xml() {
        let xml = r#"<FileZilla3>
          <RecentServers>
            <Server>
              <Host>recent.example</Host>
              <Port>21</Port>
              <Protocol>6</Protocol>
              <Logontype>1</Logontype>
              <User>demo</User>
            </Server>
          </RecentServers>
        </FileZilla3>"#;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-recent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::load(dir);
        let outcome = import_xml(&store, xml).unwrap();
        assert_eq!(outcome.imported, 1);
        assert_eq!(store.snapshot().sites[0].config.host, "recent.example");
    }
    #[test]
    fn exports_groups_and_sites() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-export");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::load(dir.clone());
        import_xml(&store, SAMPLE).unwrap();
        let path = dir.join("sitemanager.xml");
        let outcome = export_file(&store, &path, false).unwrap();
        assert_eq!(outcome.sites, 1);
        let xml = std::fs::read_to_string(&path).unwrap();
        assert!(xml.contains("<Folder name=\"Work\">"));
        assert!(xml.contains("<Host>test.rebex.net</Host>"));
        assert!(xml.contains("<Protocol>4</Protocol>"));
    }
    #[test]
    fn detects_managed_filezilla_config() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-detect");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("filezilla");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("sitemanager.xml"), SAMPLE).unwrap();

        let store = Store::load(dir.join("app"));
        let detected = detect(config.to_str()).expect("detects the site list");
        assert_eq!(detected, config.join("sitemanager.xml"));
        let outcome = import_detected(&store, config.to_str())
            .unwrap()
            .expect("detected FileZilla configuration");
        assert_eq!(outcome.imported, 1);
        assert_eq!(outcome.groups_created, 1);
        assert_eq!(outcome.sources.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn imports_nested_folder_and_password() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-import");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::load(dir);
        let outcome = import_xml(&store, SAMPLE).unwrap();
        assert_eq!(outcome.imported, 1);
        assert_eq!(outcome.groups_created, 1);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.sites.len(), 1);
        assert_eq!(snapshot.groups.len(), 1);
        assert_eq!(snapshot.sites[0].name, "Rebex Work");
        assert_eq!(snapshot.sites[0].config.host, "test.rebex.net");
        assert_eq!(snapshot.sites[0].config.encoding, "GBK");
        assert_eq!(snapshot.sites[0].config.protocol, Protocol::Ftps);
        assert_eq!(
            snapshot.sites[0].config.initial_remote_dir.as_deref(),
            Some("/pub")
        );
    }
}
