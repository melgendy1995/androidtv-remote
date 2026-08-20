use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntry {
    pub id: String,
    pub started_at: u64,
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub encrypted: bool,
    pub request_headers: std::collections::HashMap<String, String>,
    pub response_headers: std::collections::HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
}

pub struct NetworkLog {
    entries: Mutex<Vec<NetworkEntry>>,
    next_id: AtomicU64,
}

impl NetworkLog {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn list(&self) -> Vec<NetworkEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    pub fn push(&self, entry: NetworkEntry) -> NetworkEntry {
        let mut lock = self.entries.lock().unwrap();
        if let Some(existing) = lock.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry.clone();
        } else {
            lock.push(entry.clone());
            if lock.len() > 2000 {
                lock.remove(0);
            }
        }
        entry
    }

    pub fn next_id(&self) -> String {
        self.next_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    pub fn upsert_from_log(&self, entry: NetworkEntry) -> NetworkEntry {
        let mut lock = self.entries.lock().unwrap();
        if let Some(existing) = lock.iter_mut().rev().take(120).find(|e| {
            e.id.starts_with("log-") && !e.url.is_empty() && e.url == entry.url
        }) {
            if entry.status.is_some() {
                existing.status = entry.status;
            }
            if entry.duration_ms.is_some() {
                existing.duration_ms = entry.duration_ms;
            }
            if existing.method == "GET" && entry.method != "GET" {
                existing.method = entry.method.clone();
            }
            if let Some(line) = entry.request_headers.get("X-Log-Line") {
                existing
                    .request_headers
                    .insert("X-Log-Line".into(), line.clone());
            }
            return existing.clone();
        }
        lock.push(entry.clone());
        if lock.len() > 2000 {
            lock.remove(0);
        }
        entry
    }

    pub fn export_har(&self) -> Result<String> {
        let entries = self.list();
        let dir = crate::paths::default_capture_dir();
        crate::paths::ensure_dir(&dir)?;
        let path = dir.join(format!(
            "network-{}.har",
            chrono::Local::now().format("%Y-%m-%d-%H%M%S")
        ));
        let har = serde_json::json!({
            "log": {
                "version": "1.2",
                "creator": { "name": "Android TV Remote", "version": "0.1.0" },
                "entries": entries.iter().map(|e| serde_json::json!({
                    "startedDateTime": chrono::DateTime::from_timestamp_millis(e.started_at as i64)
                        .unwrap_or_else(chrono::Utc::now)
                        .to_rfc3339(),
                    "time": e.duration_ms.unwrap_or(0),
                    "request": {
                        "method": e.method,
                        "url": e.url,
                        "headers": e.request_headers.iter().map(|(name, value)| serde_json::json!({ "name": name, "value": value })).collect::<Vec<_>>(),
                        "httpVersion": "HTTP/1.1",
                    },
                    "response": {
                        "status": e.status.unwrap_or(0),
                        "statusText": if e.encrypted { "encrypted" } else { "" },
                        "headers": e.response_headers.iter().map(|(name, value)| serde_json::json!({ "name": name, "value": value })).collect::<Vec<_>>(),
                        "content": { "size": e.size.unwrap_or(0), "mimeType": "application/octet-stream", "text": e.response_body },
                        "httpVersion": "HTTP/1.1",
                    },
                    "cache": {},
                    "timings": { "send": 0, "wait": e.duration_ms.unwrap_or(0), "receive": 0 }
                })).collect::<Vec<_>>()
            }
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&har)?)?;
        Ok(path.to_string_lossy().to_string())
    }
}

pub async fn start_proxy(
    port: u16,
    log: Arc<NetworkLog>,
    on_entry: impl Fn(NetworkEntry) + Send + Sync + 'static,
) -> Result<u16> {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(_) => TcpListener::bind("127.0.0.1:0").await?,
    };
    let bound = listener.local_addr()?.port();
    let on_entry = Arc::new(on_entry);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let log = log.clone();
            let on_entry = on_entry.clone();
            tokio::spawn(async move {
                let _ = handle_client(stream, log, on_entry).await;
            });
        }
    });
    Ok(bound)
}

async fn handle_client(
    mut client: TcpStream,
    log: Arc<NetworkLog>,
    on_entry: Arc<dyn Fn(NetworkEntry) + Send + Sync>,
) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    let n = client.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let head = String::from_utf8_lossy(&buf[..n]);
    let first = head.lines().next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();

    let started = chrono::Utc::now().timestamp_millis() as u64;
    if method == "CONNECT" {
        let host = target.clone();
        let (request_headers, _) = parse_headers_and_body(&buf[..n]);
        let entry = NetworkEntry {
            id: log.next_id(),
            started_at: started,
            method,
            url: entry_url(true, &host, ""),
            host,
            path: String::new(),
            status: None,
            duration_ms: None,
            size: None,
            encrypted: true,
            request_headers,
            response_headers: Default::default(),
            request_body: None,
            response_body: None,
        };
        on_entry(log.push(entry.clone()));
        let dest = if target.contains(':') {
            target
        } else {
            format!("{target}:443")
        };
        match TcpStream::connect(&dest).await {
            Ok(mut upstream) => {
                client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await?;
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                let mut done = entry;
                done.status = Some(200);
                done.duration_ms = Some((chrono::Utc::now().timestamp_millis() as u64).saturating_sub(started));
                on_entry(log.push(done));
            }
            Err(_) => {
                let _ = client
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        }
        return Ok(());
    }

    let (host, path) = split_url(&target);
    let dest = if host.contains(':') {
        host.clone()
    } else {
        format!("{host}:80")
    };
    let (request_headers, request_body) = parse_headers_and_body(&buf[..n]);
    let mut entry = NetworkEntry {
        id: log.next_id(),
        started_at: started,
        method,
        url: entry_url(false, &host, &path),
        host: host.clone(),
        path,
        status: None,
        duration_ms: None,
        size: None,
        encrypted: false,
        request_headers,
        response_headers: Default::default(),
        request_body,
        response_body: None,
    };
    on_entry(log.push(entry.clone()));

    let mut upstream = TcpStream::connect(&dest).await?;
    let rewritten = rewrite_absolute_request(&head);
    upstream.write_all(rewritten.as_bytes()).await?;
    if n == buf.len() {
        let _ = tokio::io::copy(&mut client, &mut upstream).await;
    }
    let mut resp = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        upstream.read_to_end(&mut resp),
    )
    .await;
    if let Some(status) = parse_status(&resp) {
        entry.status = Some(status);
    }
    let (response_headers, response_body) = parse_headers_and_body(&resp);
    entry.response_headers = response_headers;
    entry.response_body = response_body;
    entry.size = Some(resp.len() as u64);
    entry.duration_ms = Some((chrono::Utc::now().timestamp_millis() as u64).saturating_sub(started));
    let _ = client.write_all(&resp).await;
    on_entry(log.push(entry));
    Ok(())
}

fn split_url(url: &str) -> (String, String) {
    if let Some(rest) = url.strip_prefix("http://") {
        if let Some((host, path)) = rest.split_once('/') {
            return (host.to_string(), format!("/{path}"));
        }
        return (rest.to_string(), "/".into());
    }
    if let Some((host, path)) = url.split_once('/') {
        (host.to_string(), format!("/{path}"))
    } else {
        (url.to_string(), "/".into())
    }
}

fn rewrite_absolute_request(head: &str) -> String {
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");
    let ver = parts.next().unwrap_or("HTTP/1.1");
    let path = if target.starts_with("http://") {
        split_url(target).1
    } else {
        target.to_string()
    };
    let mut out = format!("{method} {path} {ver}\r\n");
    for line in lines {
        if line.is_empty() {
            out.push_str("\r\n");
            break;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    if !out.ends_with("\r\n\r\n") {
        out.push_str("\r\n");
    }
    out
}

fn parse_status(resp: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(resp);
    let first = text.lines().next()?;
    first.split_whitespace().nth(1)?.parse().ok()
}

fn entry_url(encrypted: bool, host: &str, path: &str) -> String {
    let scheme = if encrypted { "https" } else { "http" };
    let host = host
        .strip_suffix(":443")
        .or_else(|| host.strip_suffix(":80"))
        .unwrap_or(host);
    if path.is_empty() {
        format!("{scheme}://{host}")
    } else if path.starts_with('/') {
        format!("{scheme}://{host}{path}")
    } else {
        format!("{scheme}://{host}/{path}")
    }
}

fn parse_headers_and_body(raw: &[u8]) -> (std::collections::HashMap<String, String>, Option<String>) {
    let text = String::from_utf8_lossy(raw);
    let Some((head, body)) = split_head_body(&text) else {
        return (std::collections::HashMap::new(), None);
    };
    let mut headers = std::collections::HashMap::new();
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_string(), value.trim().to_string());
    }
    let body = preview_body(body);
    (headers, body)
}

fn split_head_body(text: &str) -> Option<(&str, &str)> {
    if let Some(idx) = text.find("\r\n\r\n") {
        return Some((&text[..idx], &text[idx + 4..]));
    }
    if let Some(idx) = text.find("\n\n") {
        return Some((&text[..idx], &text[idx + 2..]));
    }
    None
}

fn preview_body(body: &str) -> Option<String> {
    let trimmed = body.trim_end_matches('\0').trim();
    if trimmed.is_empty() {
        return None;
    }
    const MAX: usize = 64_000;
    let count = trimmed.chars().count();
    if count <= MAX {
        return Some(trimmed.to_string());
    }
    let mut out: String = trimmed.chars().take(MAX).collect();
    out.push_str("\n… truncated");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_absolute_http_url_with_query() {
        assert_eq!(
            split_url("http://api.example.com/v1/search?q=tv"),
            ("api.example.com".into(), "/v1/search?q=tv".into())
        );
    }

    #[test]
    fn builds_https_url_without_default_port() {
        assert_eq!(
            entry_url(true, "netflix.com:443", "/browse?q=tv"),
            "https://netflix.com/browse?q=tv"
        );
    }

    #[test]
    fn parses_request_headers_and_json_body() {
        let raw = b"POST /login HTTP/1.1\r\nHost: api.tv\r\nContent-Type: application/json\r\n\r\n{\"u\":\"a\"}";
        let (headers, body) = parse_headers_and_body(raw);
        assert_eq!(headers.get("Host").map(String::as_str), Some("api.tv"));
        assert_eq!(body.as_deref(), Some("{\"u\":\"a\"}"));
    }
}
