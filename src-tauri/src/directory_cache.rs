//! Per-session directory listing cache.
//!
//! FileZilla keeps a directory cache to make back/forward navigation instant
//! and to survive short control-connection hiccups.  This implementation is
//! deliberately small: fresh listings are served from memory, mutations clear
//! the cache, and stale entries are only used as a fallback when a live listing
//! fails.

use crate::models::RemoteEntry;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct DirectoryCache {
    ttl: Duration,
    entries: HashMap<String, CachedListing>,
}

#[derive(Debug)]
struct CachedListing {
    at: Instant,
    entries: Vec<RemoteEntry>,
}

impl DirectoryCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn get(&self, path: &str) -> Option<Vec<RemoteEntry>> {
        let cached = self.entries.get(path)?;
        if cached.at.elapsed() <= self.ttl {
            Some(cached.entries.clone())
        } else {
            None
        }
    }

    /// Return a listing regardless of age; used only when the live request
    /// failed.
    pub fn get_stale(&self, path: &str) -> Option<Vec<RemoteEntry>> {
        self.entries.get(path).map(|cached| cached.entries.clone())
    }

    pub fn put(&mut self, path: &str, entries: Vec<RemoteEntry>) {
        self.entries.insert(
            path.to_string(),
            CachedListing {
                at: Instant::now(),
                entries,
            },
        );
    }

    #[allow(dead_code)]
    pub fn invalidate(&mut self, path: &str) {
        self.entries.remove(path);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> RemoteEntry {
        RemoteEntry {
            name: name.into(),
            path: format!("/{name}"),
            is_dir: false,
            is_symlink: false,
            size: 1,
            modified: None,
            permissions: None,
            mode: None,
            owner: None,
            group: None,
        }
    }

    #[test]
    fn stores_and_invalidates() {
        let mut cache = DirectoryCache::new(Duration::from_secs(30));
        cache.put("/", vec![entry("a")]);
        assert_eq!(cache.get("/").unwrap()[0].name, "a");
        assert_eq!(cache.len(), 1);
        cache.invalidate("/");
        assert!(cache.get("/").is_none());
        assert!(cache.get_stale("/").is_none());
    }

    #[test]
    fn stale_entries_survive_ttl() {
        let mut cache = DirectoryCache::new(Duration::ZERO);
        cache.put("/pub", vec![entry("x")]);
        assert!(cache.get("/pub").is_none());
        assert_eq!(cache.get_stale("/pub").unwrap()[0].name, "x");
        cache.clear();
        assert!(cache.get_stale("/pub").is_none());
    }
}
