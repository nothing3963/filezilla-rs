//! Live connection registry: one session per browser tab.

use crate::directory_cache::DirectoryCache;
use crate::error::{AppError, AppResult};
use crate::models::{ConnectRequest, RemoteEntry, SessionInfo, SiteConfig};
use crate::remote::{self, RemoteFs, ResolvedConfig};
use crate::secrets;
use crate::store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

pub struct Session {
    #[allow(dead_code)]
    pub info: SessionInfo,
    pub resolved: ResolvedConfig,
    pub fs: Mutex<Box<dyn RemoteFs>>,
    pub last_used: Mutex<Instant>,
    keepalive: Mutex<Option<tokio::task::JoinHandle<()>>>,
    detected_encoding: Mutex<Option<String>>,
    cache: Mutex<DirectoryCache>,
}

impl Session {
    pub async fn touch(&self) {
        *self.last_used.lock().await = Instant::now();
    }
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    store: Arc<Store>,
}

impl SessionManager {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            store,
        }
    }

    pub async fn connect(&self, request: ConnectRequest) -> AppResult<SessionInfo> {
        let (config, name, site_id, stored_password) = self.resolve(&request)?;
        let proxy_password = request
            .credentials
            .as_ref()
            .and_then(|creds| creds.proxy_password.clone())
            .filter(|value| !value.is_empty())
            .or_else(|| site_id.as_deref().and_then(secrets::get_proxy_password));
        let (password, key_passphrase) = match &request.credentials {
            Some(creds) => (
                creds
                    .password
                    .clone()
                    .filter(|p| !p.is_empty())
                    .or(stored_password),
                creds
                    .key_passphrase
                    .clone()
                    .filter(|p| !p.is_empty())
                    .or_else(|| site_id.as_deref().and_then(secrets::get_key_passphrase)),
            ),
            None => (
                stored_password,
                site_id.as_deref().and_then(secrets::get_key_passphrase),
            ),
        };

        let resolved = ResolvedConfig {
            site_id: site_id.clone(),
            name: name.clone(),
            config: config.clone(),
            password: password.clone(),
            key_passphrase: key_passphrase.clone(),
            proxy_password,
            store: self.store.clone(),
        };

        let t0 = Instant::now();
        let mut fs = remote::connect(&resolved).await?;
        let home = fs.realpath(".").await.unwrap_or_else(|_| "/".to_string());
        let info = SessionInfo {
            session_id: Uuid::new_v4().to_string(),
            protocol: config.protocol,
            host: config.host.clone(),
            port: config.effective_port(),
            username: config.username.clone(),
            name,
            home_dir: home,
            banner: fs.banner(),
            features: fs.features(),
            site_id: site_id.clone(),
        };

        // Persist a password only after a successful login.
        if request.save_password && self.store.kiosk_mode() == 0 {
            if let (Some(id), Some(pw)) = (site_id.as_deref(), password.as_deref()) {
                let _ = secrets::set_password(id, pw);
                if let Some(pass) = key_passphrase.as_deref() {
                    let _ = secrets::set_key_passphrase(id, pass);
                }
            }
        }

        tracing::info!(
            "connected to {} as {} ({} ms)",
            config.endpoint(),
            config.username,
            t0.elapsed().as_millis()
        );

        let session = Arc::new(Session {
            info: info.clone(),
            resolved,
            fs: Mutex::new(fs),
            last_used: Mutex::new(Instant::now()),
            keepalive: Mutex::new(None),
            detected_encoding: Mutex::new(None),
            cache: Mutex::new(DirectoryCache::new(Duration::from_secs(30))),
        });

        if config.keepalive_secs > 0 {
            let interval = Duration::from_secs(config.keepalive_secs.max(15));
            let weak = Arc::downgrade(&session);
            let handle = tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    let Some(session) = weak.upgrade() else {
                        break;
                    };
                    if let Ok(mut fs) = session.fs.try_lock() {
                        if fs.keepalive().await.is_err() {
                            break;
                        }
                    };
                }
            });
            *session.keepalive.lock().await = Some(handle);
        }

        self.sessions
            .write()
            .await
            .insert(info.session_id.clone(), session);
        Ok(info)
    }

    fn resolve(
        &self,
        request: &ConnectRequest,
    ) -> AppResult<(SiteConfig, String, Option<String>, Option<String>)> {
        if let Some(site_id) = request.site_id.as_deref() {
            let site = self
                .store
                .site(site_id)
                .ok_or_else(|| AppError::not_found("Site not found"))?;
            let password = secrets::get_password(site_id);
            return Ok((site.config, site.name, Some(site.id), password));
        }
        let mut config = request
            .config
            .clone()
            .ok_or_else(|| AppError::invalid("Connection configuration is missing"))?;
        config.validate()?;
        let name = request
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| config.host.clone());
        Ok((config, name, None, None))
    }

    pub async fn get(&self, session_id: &str) -> AppResult<Arc<Session>> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::new("SESSION", "This connection is no longer open"))
    }

    pub async fn list_remote(&self, session_id: &str, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.list(path).await;
        let encoding = fs.encoding();
        drop(fs);
        if let Some(encoding) = encoding {
            *session.detected_encoding.lock().await = Some(encoding);
        }
        session.touch().await;
        match &result {
            Ok(entries) => {
                session.cache.lock().await.put(path, entries.clone());
            }
            Err(_) => {
                if let Some(cached) = session.cache.lock().await.get_stale(path) {
                    return Ok(cached);
                }
            }
        }
        result
    }

    pub async fn mkdir(&self, session_id: &str, path: &str) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.mkdir(path).await;
        drop(fs);
        session.touch().await;
        if result.is_ok() {
            session.cache.lock().await.clear();
        }
        result
    }

    pub async fn remove_file(&self, session_id: &str, path: &str) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.remove_file(path).await;
        drop(fs);
        session.touch().await;
        if result.is_ok() {
            session.cache.lock().await.clear();
        }
        result
    }

    pub async fn remove_dir(&self, session_id: &str, path: &str) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.remove_dir(path).await;
        drop(fs);
        session.touch().await;
        if result.is_ok() {
            session.cache.lock().await.clear();
        }
        result
    }

    pub async fn rename(&self, session_id: &str, from: &str, to: &str) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.rename(from, to).await;
        drop(fs);
        session.touch().await;
        if result.is_ok() {
            session.cache.lock().await.clear();
        }
        result
    }

    pub async fn chmod(&self, session_id: &str, path: &str, mode: u32) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.chmod(path, mode).await;
        drop(fs);
        session.touch().await;
        if result.is_ok() {
            session.cache.lock().await.clear();
        }
        result
    }

    pub async fn stat(&self, session_id: &str, path: &str) -> AppResult<RemoteEntry> {
        let session = self.get(session_id).await?;
        let mut fs = session.fs.lock().await;
        let result = fs.stat(path).await;
        drop(fs);
        session.touch().await;
        result
    }

    pub async fn resolved_for(&self, session_id: &str) -> AppResult<ResolvedConfig> {
        let session = self.get(session_id).await?;
        let mut resolved = session.resolved.clone();
        if let Some(encoding) = session.detected_encoding.lock().await.clone() {
            resolved.config.encoding = encoding;
        }
        Ok(resolved)
    }

    pub async fn disconnect(&self, session_id: &str) -> AppResult<()> {
        let session = self.sessions.write().await.remove(session_id);
        if let Some(session) = session {
            if let Some(handle) = session.keepalive.lock().await.take() {
                handle.abort();
            }
            let mut fs = session.fs.lock().await;
            let _ = fs.close().await;
            tracing::info!("disconnected session {session_id}");
            Ok(())
        } else {
            Err(AppError::new(
                "SESSION",
                "This connection is no longer open",
            ))
        }
    }

    pub async fn close_all(&self) {
        let sessions: Vec<Arc<Session>> = {
            let mut map = self.sessions.write().await;
            map.drain().map(|(_, v)| v).collect()
        };
        for session in sessions {
            if let Some(handle) = session.keepalive.lock().await.take() {
                handle.abort();
            }
            let mut fs = session.fs.lock().await;
            let _ = fs.close().await;
        }
    }
}
