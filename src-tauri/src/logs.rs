//! In-app activity log. Entries are emitted to the UI and kept in a small
//! in-memory ring buffer so the log panel can be opened later.

use crate::models::LogEntry;
use chrono::Utc;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

const CAPACITY: usize = 2000;

pub struct LogBus {
    app: AppHandle,
    counter: AtomicU64,
    entries: Mutex<VecDeque<LogEntry>>,
}

impl LogBus {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            counter: AtomicU64::new(1),
            entries: Mutex::new(VecDeque::with_capacity(256)),
        }
    }

    pub fn push(&self, level: &str, scope: &str, message: impl Into<String>) {
        let entry = LogEntry {
            id: self.counter.fetch_add(1, Ordering::Relaxed),
            timestamp: Utc::now().timestamp_millis(),
            level: level.to_string(),
            scope: scope.to_string(),
            message: message.into(),
        };
        match level {
            "error" => tracing::error!(target: "filezilla", "[{}] {}", scope, entry.message),
            "warn" => tracing::warn!(target: "filezilla", "[{}] {}", scope, entry.message),
            "debug" => tracing::debug!(target: "filezilla", "[{}] {}", scope, entry.message),
            _ => tracing::info!(target: "filezilla", "[{}] {}", scope, entry.message),
        }
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if entries.len() >= CAPACITY {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        let _ = self.app.emit("log:entry", entry);
    }

    pub fn info(&self, scope: &str, message: impl Into<String>) {
        self.push("info", scope, message);
    }

    pub fn warn(&self, scope: &str, message: impl Into<String>) {
        self.push("warn", scope, message);
    }

    pub fn error(&self, scope: &str, message: impl Into<String>) {
        self.push("error", scope, message);
    }

    pub fn debug(&self, scope: &str, message: impl Into<String>) {
        self.push("debug", scope, message);
    }

    pub fn history(&self) -> Vec<LogEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}
