use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adb::AdbClient;
use crate::error::Result;
use crate::logcat::LogLine;
use crate::paths::{default_capture_dir, ensure_dir};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashEntry {
    pub id: String,
    pub at: u64,
    pub kind: String,
    pub process: String,
    pub reason: String,
    pub stack: String,
}

pub struct CrashLog {
    entries: Mutex<Vec<CrashEntry>>,
}

impl CrashLog {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    pub fn list(&self) -> Vec<CrashEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<CrashEntry> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }

    pub fn push(&self, kind: &str, process: String, reason: String, stack: String) -> CrashEntry {
        let entry = CrashEntry {
            id: format!("{}", now_ms()),
            at: now_ms(),
            kind: kind.to_string(),
            process,
            reason,
            stack,
        };
        let mut lock = self.entries.lock().unwrap();
        lock.insert(0, entry.clone());
        lock.truncate(200);
        entry
    }

    pub fn save(&self, id: &str) -> Result<String> {
        let entry = self
            .get(id)
            .ok_or_else(|| crate::error::AppError::from("crash not found"))?;
        let dir = default_capture_dir();
        ensure_dir(&dir)?;
        let path = dir.join(format!("{}-{}.txt", entry.kind, entry.id));
        let body = format!(
            "{} {} {}\n{}\n\n{}\n",
            entry.kind, entry.process, entry.at, entry.reason, entry.stack
        );
        std::fs::write(&path, body)?;
        Ok(path.to_string_lossy().to_string())
    }
}

pub async fn enrich_from_device(adb: &AdbClient, serial: &str, line: &LogLine) -> String {
    let mut stack = line.message.clone();
    if let Ok(dropbox) = adb.shell(serial, "dumpsys dropbox --print").await {
        let snippet: String = dropbox.chars().rev().take(8000).collect::<String>().chars().rev().collect();
        if snippet.contains("data_app_crash")
            || snippet.contains("data_app_anr")
            || snippet.contains("system_app_crash")
        {
            stack.push_str("\n\n--- dropbox ---\n");
            stack.push_str(&snippet);
        }
    }
    if let Ok(anr) = adb.shell(serial, "ls /data/anr 2>/dev/null").await {
        if !anr.trim().is_empty() {
            stack.push_str("\n\n--- /data/anr ---\n");
            stack.push_str(anr.trim());
        }
    }
    stack
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
