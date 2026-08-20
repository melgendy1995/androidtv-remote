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
    #[serde(default)]
    pub raw: String,
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
        raw: raw.to_string(),
    })
}

pub fn parse_threadtime(raw: &str, id: u64) -> Option<LogLine> {
    let raw_trimmed = raw.trim();
    if raw_trimmed.is_empty() {
        return None;
    }
    if let Some(mut line) = try_parse_threadtime(raw_trimmed, id) {
        line.raw = raw_trimmed.to_string();
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
        raw: raw_trimmed.to_string(),
    })
}

pub fn is_crash_line(line: &LogLine) -> Option<(&'static str, String, String)> {
    let msg = format!("{} {}", line.tag, line.message);
    let lower = msg.to_lowercase();
    if lower.contains("fatal exception")
        || line.tag == "AndroidRuntime" && lower.contains("fatal")
        || lower.contains("am_crash")
        || lower.contains("fatal signal")
        || lower.contains("native crash")
    {
        return Some(("crash", line.tag.clone(), line.message.clone()));
    }
    if lower.contains("anr in")
        || lower.contains("am_anr")
        || (line.tag == "ActivityManager" && lower.contains("anr"))
    {
        return Some(("anr", line.tag.clone(), line.message.clone()));
    }
    None
}

pub fn is_stack_followup(line: &LogLine) -> bool {
    let tag = line.tag.as_str();
    if matches!(
        tag,
        "AndroidRuntime" | "DEBUG" | "libc" | "tombstoned" | "CrashAnrDetector" | "ActivityManager"
    ) {
        return true;
    }
    let m = line.message.trim_start();
    m.starts_with("at ")
        || m.starts_with("Caused by:")
        || m.starts_with("Process:")
        || m.starts_with("PID:")
        || m.starts_with("UID:")
        || m.starts_with("signal ")
        || m.starts_with("Abort message")
        || m.starts_with("backtrace:")
        || m.starts_with('#')
        || m.contains("java.")
        || m.contains("kotlin.")
}

#[derive(Debug, Clone)]
pub struct LoggedHttp {
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub encrypted: bool,
}

pub fn extract_http(line: &LogLine) -> Option<LoggedHttp> {
    let url = find_http_url(&line.message)?;
    let encrypted = url.starts_with("https://");
    let (host, path) = split_host_path(&url);
    if host.contains("schemas.android.com") || host.contains("w3.org") {
        return None;
    }
    let method = find_http_method(&line.message).unwrap_or_else(|| "GET".into());
    let status = find_http_status(&line.message);
    let duration_ms = find_duration_ms(&line.message);
    Some(LoggedHttp {
        method,
        url,
        host,
        path,
        status,
        duration_ms,
        encrypted,
    })
}

fn find_http_url(text: &str) -> Option<String> {
    let start = text.find("https://").or_else(|| text.find("http://"))?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| {
            c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']' | ',')
        })
        .unwrap_or(rest.len());
    let mut url = rest[..end].to_string();
    while url.ends_with(['.', ';', ':', '"', '\'']) {
        url.pop();
    }
    if url.len() < 10 {
        return None;
    }
    Some(url)
}

fn split_host_path(url: &str) -> (String, String) {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    if let Some((host, path)) = rest.split_once('/') {
        (host.to_string(), format!("/{path}"))
    } else if let Some((host, query)) = rest.split_once('?') {
        (host.to_string(), format!("/?{query}"))
    } else {
        (rest.to_string(), "/".into())
    }
}

fn find_http_method(text: &str) -> Option<String> {
    for method in ["POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD", "GET", "CONNECT"] {
        if let Some(idx) = text.find(method) {
            let after = idx + method.len();
            if after >= text.len() || text.as_bytes()[after].is_ascii_whitespace() {
                return Some(method.to_string());
            }
        }
    }
    None
}

fn find_http_status(text: &str) -> Option<u16> {
    let window = if let Some(idx) = text.find("<--") {
        &text[idx..]
    } else if let Some(idx) = text.find("HTTP/") {
        &text[idx..]
    } else {
        return None;
    };
    for token in window.split(|c: char| !c.is_ascii_digit()) {
        if token.len() == 3 {
            if let Ok(code) = token.parse::<u16>() {
                if (100..600).contains(&code) {
                    return Some(code);
                }
            }
        }
    }
    None
}

fn find_duration_ms(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    if let Some(idx) = lower.find("ms") {
        let before = &text[..idx];
        let digits: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_okhttp_full_url_with_query() {
        let line = LogLine {
            id: 1,
            time: "01-01 12:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: "D".into(),
            tag: "OkHttp".into(),
            message: "--> GET https://api.intigral-ott.net/cms/v1/page?id=12&lang=en http/1.1"
                .into(),
            raw: String::new(),
        };
        let http = extract_http(&line).expect("url");
        assert_eq!(http.method, "GET");
        assert_eq!(
            http.url,
            "https://api.intigral-ott.net/cms/v1/page?id=12&lang=en"
        );
        assert_eq!(http.path, "/cms/v1/page?id=12&lang=en");
        assert!(http.encrypted);
    }

    #[test]
    fn extracts_status_and_duration() {
        let line = LogLine {
            id: 2,
            time: String::new(),
            pid: 1,
            tid: 1,
            level: "D".into(),
            tag: "OkHttp".into(),
            message: "<-- 200 OK https://api.example.com/v2/foo (145ms)".into(),
            raw: String::new(),
        };
        let http = extract_http(&line).expect("url");
        assert_eq!(http.status, Some(200));
        assert_eq!(http.duration_ms, Some(145));
    }
}
