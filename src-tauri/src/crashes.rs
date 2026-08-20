use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adb::AdbClient;
use crate::error::Result;
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
    #[serde(default)]
    pub pid: i32,
    #[serde(default)]
    pub exception: String,
    #[serde(default)]
    pub package_name: String,
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

    pub fn push(
        &self,
        kind: &str,
        process: String,
        reason: String,
        stack: String,
        pid: i32,
    ) -> CrashEntry {
        let (package_name, exception) = parse_crash_meta(&process, &reason, &stack);
        let process = if package_name.is_empty() {
            process
        } else {
            package_name.clone()
        };
        let entry = CrashEntry {
            id: format!("{}", now_ms()),
            at: now_ms(),
            kind: kind.to_string(),
            process,
            reason,
            stack,
            pid,
            exception,
            package_name,
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
            "kind: {}\nprocess: {}\npackage: {}\npid: {}\nexception: {}\ntime: {}\nreason: {}\n\n{}\n",
            entry.kind,
            entry.process,
            entry.package_name,
            entry.pid,
            entry.exception,
            entry.at,
            entry.reason,
            entry.stack
        );
        std::fs::write(&path, body)?;
        Ok(path.to_string_lossy().to_string())
    }
}

pub async fn enrich_from_device(adb: &AdbClient, serial: &str, collected: &str) -> String {
    let mut stack = collected.to_string();
    if let Ok(crash_buf) = adb
        .run_serial_timeout(
            Some(serial),
            &["logcat", "-d", "-b", "crash", "-t", "200"],
            std::time::Duration::from_secs(8),
        )
        .await
    {
        let trimmed = crash_buf.trim();
        if !trimmed.is_empty() && !stack.contains(trimmed) {
            stack.push_str("\n\n--- logcat crash buffer ---\n");
            stack.push_str(trimmed);
        }
    }
    if let Ok(dropbox) = adb.shell(serial, "dumpsys dropbox --print").await {
        if let Some(snippet) = last_crash_block(&dropbox) {
            if !stack.contains(&snippet) {
                stack.push_str("\n\n--- dropbox ---\n");
                stack.push_str(&snippet);
            }
        }
    }
    if let Ok(anr) = adb.shell(serial, "ls -l /data/anr 2>/dev/null").await {
        if !anr.trim().is_empty() {
            stack.push_str("\n\n--- /data/anr ---\n");
            stack.push_str(anr.trim());
        }
    }
    stack
}

fn last_crash_block(s: &str) -> Option<String> {
    let idx = s
        .rmatch_indices("FATAL EXCEPTION")
        .map(|(i, _)| i)
        .next()
        .or_else(|| s.rmatch_indices("data_app_crash").map(|(i, _)| i).next())
        .or_else(|| s.rmatch_indices("data_app_anr").map(|(i, _)| i).next())?;
    let start = s[..idx].rfind('\n').map(|i| i + 1).unwrap_or(idx);
    let mut end = (start + 16_000).min(s.len());
    while end > start && !s.is_char_boundary(end) {
        end -= 1;
    }
    if let Some(nl) = s[end..].find('\n') {
        end += nl;
    }
    Some(s[start..end].to_string())
}

fn parse_crash_meta(process: &str, reason: &str, stack: &str) -> (String, String) {
    let mut package = String::new();
    let mut exception = String::new();
    for line in stack.lines().chain(std::iter::once(reason)) {
        let t = line.trim();
        if package.is_empty() {
            if let Some(rest) = t.strip_prefix("Process:") {
                package = rest.split(',').next().unwrap_or("").trim().to_string();
            }
        }
        if exception.is_empty() {
            if let Some(idx) = t.find("Exception") {
                let start = t[..idx]
                    .rfind(|c: char| c.is_whitespace())
                    .map(|i| i + 1)
                    .unwrap_or(0);
                exception = t[start..].split(':').next().unwrap_or("").trim().to_string();
            } else if t.starts_with("FATAL EXCEPTION") {
                exception = t.to_string();
            }
        }
    }
    if package.is_empty() && process.contains('.') {
        package = process.to_string();
    }
    (package, exception)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
