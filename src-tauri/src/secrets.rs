//! Secret storage.
//!
//! Passwords and key passphrases are kept in the OS credential vault
//! (Windows Credential Manager, macOS Keychain, libsecret on Linux) and never
//! written to the JSON site store.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "FileZilla Modern";

fn entry(site_id: &str, kind: &str) -> AppResult<keyring::Entry> {
    let user = format!("site:{site_id}:{kind}");
    keyring::Entry::new(SERVICE, &user)
        .map_err(|e| AppError::new("SECRET", format!("Credential store unavailable: {e}")))
}

pub fn set_password(site_id: &str, password: &str) -> AppResult<()> {
    entry(site_id, "password")?
        .set_password(password)
        .map_err(|e| AppError::new("SECRET", format!("Cannot save password: {e}")))
}

pub fn get_password(site_id: &str) -> Option<String> {
    entry(site_id, "password").ok()?.get_password().ok()
}

pub fn delete_password(site_id: &str) -> AppResult<()> {
    match entry(site_id, "password")?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::new(
            "SECRET",
            format!("Cannot remove password: {e}"),
        )),
    }
}

pub fn set_key_passphrase(site_id: &str, passphrase: &str) -> AppResult<()> {
    entry(site_id, "key")?
        .set_password(passphrase)
        .map_err(|e| AppError::new("SECRET", format!("Cannot save key passphrase: {e}")))
}

pub fn get_key_passphrase(site_id: &str) -> Option<String> {
    entry(site_id, "key").ok()?.get_password().ok()
}

pub fn delete_key_passphrase(site_id: &str) -> AppResult<()> {
    match entry(site_id, "key")?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::new(
            "SECRET",
            format!("Cannot remove key passphrase: {e}"),
        )),
    }
}

pub fn set_proxy_password(site_id: &str, password: &str) -> AppResult<()> {
    entry(site_id, "proxy")?
        .set_password(password)
        .map_err(|e| AppError::new("SECRET", format!("Cannot save proxy password: {e}")))
}

pub fn get_proxy_password(site_id: &str) -> Option<String> {
    entry(site_id, "proxy").ok()?.get_password().ok()
}

pub fn delete_proxy_password(site_id: &str) -> AppResult<()> {
    match entry(site_id, "proxy")?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::new(
            "SECRET",
            format!("Cannot remove proxy password: {e}"),
        )),
    }
}

pub fn delete_all(site_id: &str) -> AppResult<()> {
    delete_password(site_id)?;
    delete_key_passphrase(site_id)?;
    delete_proxy_password(site_id)
}
