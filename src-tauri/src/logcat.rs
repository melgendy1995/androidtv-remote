use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;

use crate::adb::AdbClient;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub id: u64,
    pub time: String,
    pub pid: i32,
    pub tid: i32,
    pub level: String,
    pub tag: String,
    pub message: String,
}

pub struct LogcatHub {
    tx: broadcast::Sender<LogLine>,
    next_id: AtomicU64,
}

impl LogcatHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(4096);
        Self {
            tx,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    pub fn push_raw(&self, raw: &str) -> Option<LogLine> {
        let line = parse_threadtime(raw, self.next_id.fetch_add(1, Ordering::Relaxed))?;
        let _ = self.tx.send(line.clone());
        Some(line)
    }
}

pub async fn spawn_logcat(
    adb: AdbClient,
    serial: String,
    hub: std::sync::Arc<LogcatHub>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut child = adb.spawn_logcat(&serial)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| crate::error::AppError::from("logcat stdout missing"))?;
    let mut reader = BufReader::new(stdout).lines();
    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    let _ = child.kill().await;
                    break;
                }
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(raw)) => {
                        hub.push_raw(&raw);
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }
    let _ = child.kill().await;
    Ok(())
}

fn try_parse_threadtime(raw: &str, id: u64) -> Option<LogLine> {
    if raw.len() < 30 {
        return None;
    }
    let time = raw.get(0..18)?.trim().to_string();
    let rest = raw.get(18..)?.trim_start();
    let mut parts = rest.split_whitespace();
    let pid: i32 = parts.next()?.parse().ok()?;
    let tid: i32 = parts.next()?.parse().unwrap_or(0);
    let level = parts.next()?.to_string();
    if !matches!(level.as_str(), "V" | "D" | "I" | "W" | "E" | "F") {
        return None;
    }
    
    let remainder = parts.collect::<Vec<&str>>().join(" ");
    let (tag, message) = remainder
        .split_once(':')
        .map(|(t, m)| (t.trim().to_string(), m.trim().to_string()))
        .unwrap_or_else(|| ("System".to_string(), remainder));

    Some(LogLine {
        id,
        time,
        pid,
        tid,
        level,
        tag,
        message,
    })
}

pub fn parse_threadtime(raw: &str, id: u64) -> Option<LogLine> {
    let raw_trimmed = raw.trim();
    if raw_trimmed.is_empty() {
        return None;
    }
    if let Some(line) = try_parse_threadtime(raw_trimmed, id) {
        return Some(line);
    }
    Some(LogLine {
        id,
        time: String::new(),
        pid: 0,
        tid: 0,
        level: "I".to_string(),
        tag: "system".to_string(),
        message: raw_trimmed.to_string(),
    })
}

pub fn is_crash_line(line: &LogLine) -> Option<(&'static str, String, String)> {
    let msg = format!("{} {}", line.tag, line.message);
    let lower = msg.to_lowercase();
    if lower.contains("fatal exception")
        || line.tag == "AndroidRuntime" && lower.contains("fatal")
        || lower.contains("am_crash")
    {
        return Some(("crash", line.tag.clone(), line.message.clone()));
    }
    if lower.contains("anr in") || lower.contains("am_anr") || line.tag == "ActivityManager" && lower.contains("anr")
    {
        return Some(("anr", line.tag.clone(), line.message.clone()));
    }
    None
}
