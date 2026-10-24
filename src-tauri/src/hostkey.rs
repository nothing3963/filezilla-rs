//! Interactive SSH host-key confirmation.
//!
//! The SFTP handler calls [`ask`] for an unknown key.  The event is delivered
//! to the React UI, which shows the fingerprint and resolves the pending
//! request.  A timeout rejects the key, so an unattended connection never
//! silently trusts a new host.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyPrompt {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
}

#[derive(Default)]
struct Pending {
    app: OnceLock<AppHandle>,
    requests: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

fn pending() -> &'static Pending {
    static PENDING: OnceLock<Pending> = OnceLock::new();
    PENDING.get_or_init(Pending::default)
}

pub fn init(app: AppHandle) {
    let _ = pending().app.set(app);
}

pub async fn ask(host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool {
    let id = format!("{host}:{port}|{key_type}");
    let Some(app) = pending().app.get() else {
        return false;
    };
    let (tx, rx) = oneshot::channel();
    pending().requests.lock().unwrap().insert(id.clone(), tx);
    let prompt = HostKeyPrompt {
        id: id.clone(),
        host: host.to_string(),
        port,
        key_type: key_type.to_string(),
        fingerprint: fingerprint.to_string(),
    };
    if app.emit("ssh:hostkey", prompt).is_err() {
        pending().requests.lock().unwrap().remove(&id);
        return false;
    }
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(accepted)) => accepted,
        _ => {
            pending().requests.lock().unwrap().remove(&id);
            false
        }
    }
}

pub fn resolve(id: &str, accepted: bool) -> bool {
    let sender = pending().requests.lock().unwrap().remove(id);
    sender.map(|tx| tx.send(accepted).is_ok()).unwrap_or(false)
}
