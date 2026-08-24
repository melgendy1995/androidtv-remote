use crate::error::Result;
use crate::mitm_ca::MitmCa;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_error: Option<String>,
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
    /// Hosts that already have a recorded TLS-failure entry — later failures
    /// for the same host are logged to the console only.
    failed_hosts: Mutex<std::collections::HashSet<String>>,
}

/// Well-known OS / Google service endpoints that flood the proxy from every
/// Android device and reject MITM certs. Never shown as network entries.
const SYSTEM_HOST_SUFFIXES: &[&str] = &[
    "google.com",
    "gstatic.com",
    "googleapis.com",
    "googleusercontent.com",
    "gvt1.com",
    "gvt2.com",
    "android.com",
    "doubleclick.net",
    "crashlytics.com",
    "firebaseio.com",
    "cloudflare.com",
    "apple.com",
    "mzstatic.com",
    "icloud.com",
    "windowsupdate.com",
    "msftconnecttest.com",
    "msn.com",
];

fn is_system_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    SYSTEM_HOST_SUFFIXES
        .iter()
        .any(|s| h == *s || h.ends_with(&format!(".{s}")))
}

impl NetworkLog {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            failed_hosts: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// True only for the first TLS failure of a host, so one bad host cannot
    /// flood the list with hundreds of red rows.
    fn should_record_tls_failure(&self, host: &str) -> bool {
        let mut set = self.failed_hosts.lock().unwrap();
        set.insert(host.to_string())
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
    ca: Arc<MitmCa>,
    on_entry: impl Fn(NetworkEntry) + Send + Sync + 'static,
) -> Result<u16> {
    let _ = rustls::crypto::ring::default_provider().install_default();
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
            let ca = ca.clone();
            tokio::spawn(async move {
                let _ = handle_client(stream, log, on_entry, ca).await;
            });
        }
    });
    Ok(bound)
}

const MAX_HTTP_MESSAGE: usize = 16 * 1024 * 1024;

/// Read one full HTTP message (head + Content-Length or chunked body) from
/// any stream (plain TCP or TLS). A single `read()` call truncates requests
/// that arrive across multiple TCP segments, so loop until the declared
/// length is satisfied.
async fn read_http_message<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>> {
    let mut data: Vec<u8> = Vec::with_capacity(16 * 1024);
    let mut buf = vec![0u8; 8192];
    let mut expected: Option<usize> = None;
    let mut chunked = false;
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.len() > MAX_HTTP_MESSAGE {
            break;
        }
        if expected.is_none() && !chunked {
            if let Some(head_end) = find_head_end(&data) {
                let head = String::from_utf8_lossy(&data[..head_end]);
                if header_value(&head, "transfer-encoding")
                    .map(|v| v.to_ascii_lowercase().contains("chunked"))
                    .unwrap_or(false)
                {
                    chunked = true;
                } else {
                    expected = Some(head_end
                        + 4
                        + header_value(&head, "content-length")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0));
                }
            }
        }
        if chunked {
            // Terminal chunk followed by the final CRLF.
            if data.ends_with(b"0\r\n\r\n") || data.windows(5).any(|w| w == b"\r\n0\r\n\r\n") {
                break;
            }
        } else if let Some(total) = expected {
            if data.len() >= total {
                break;
            }
        }
    }
    Ok(data)
}

fn find_head_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w == b"\r\n\r\n")
}

fn header_value<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

async fn handle_client(
    mut client: TcpStream,
    log: Arc<NetworkLog>,
    on_entry: Arc<dyn Fn(NetworkEntry) + Send + Sync>,
    ca: Arc<MitmCa>,
) -> Result<()> {
    let raw = read_http_message(&mut client).await?;
    if raw.is_empty() {
        return Ok(());
    }
    let head_end = find_head_end(&raw).unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let first = head.lines().next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();

    let started = chrono::Utc::now().timestamp_millis() as u64;
    if method == "CONNECT" {
        // OS / Google service chatter: tunnel opaquely, capture nothing.
        if is_system_host(
            target.split(':').next().unwrap_or(&target),
        ) {
            let dest = if target.contains(':') {
                target.clone()
            } else {
                format!("{target}:443")
            };
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
            return match TcpStream::connect(&dest).await {
                Ok(mut upstream) => {
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                    Ok(())
                }
                Err(_) => {
                    let _ = client
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    Ok(())
                }
            };
        }

        // HTTPS: terminate TLS ourselves (MITM), capture the inner requests.
        let host = target
            .split(':')
            .next()
            .unwrap_or(&target)
            .trim_end_matches('.')
            .to_string();
        let (request_headers, _) = parse_headers_and_body(&raw);
        let entry = NetworkEntry {
            id: log.next_id(),
            started_at: started,
            method,
            url: entry_url(true, &host, ""),
            host: host.clone(),
            path: String::new(),
            status: None,
            duration_ms: None,
            size: None,
            encrypted: true,
            tls_error: None,
            request_headers,
            response_headers: Default::default(),
            request_body: None,
            response_body: None,
        };
        on_entry(log.push(entry.clone()));
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;

        match ca.server_config(&host) {
            Some(config) => {
                let acceptor = TlsAcceptor::from(config);
                match acceptor.accept(client).await {
                    Ok(mut tls) => {
                        loop {
                            let raw = match tokio::time::timeout(
                                std::time::Duration::from_secs(120),
                                read_http_message(&mut tls),
                            )
                            .await
                            {
                                Ok(Ok(raw)) => raw,
                                _ => break,
                            };
                            if raw.is_empty() {
                                break;
                            }
                            if serve_https_request(&mut tls, &host, raw, &log, &on_entry)
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        let mut done = entry;
                        done.status = Some(200);
                        done.duration_ms =
                            Some((chrono::Utc::now().timestamp_millis() as u64).saturating_sub(started));
                        on_entry(log.push(done));
                    }
                    Err(e) => record_tls_failure(&host, e, &log, &on_entry, started),
                }
            }
            None => {
                let mut failed = entry;
                failed.tls_error = Some("no MITM CA available — TLS tunnel left opaque".into());
                on_entry(log.push(failed));
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
    let (request_headers, request_body) = parse_headers_and_body(&raw);
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
        tls_error: None,
        request_headers,
        response_headers: Default::default(),
        request_body,
        response_body: None,
    };
    on_entry(log.push(entry.clone()));

    let mut upstream = TcpStream::connect(&dest).await?;
    // Force connection close upstream so the response is EOF-delimited and
    // read_to_end returns promptly instead of stalling on keep-alive.
    let rewritten = rewrite_request_head(&head);
    upstream.write_all(rewritten.as_bytes()).await?;
    upstream.write_all(&raw[head_end + 4..]).await?;
    let mut resp = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        upstream.read_to_end(&mut resp),
    )
    .await;
    if let Some(status) = parse_status(&resp) {
        entry.status = Some(status);
    }
    let resp_head_end = find_head_end(&resp).unwrap_or(resp.len());
    let resp_head = String::from_utf8_lossy(&resp[..resp_head_end]).to_string();
    entry.response_headers = headers_from_head(&resp_head);
    entry.response_body = display_response_body(&resp_head, &resp[resp_head_end + 4..]);
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

/// Rewrite a request head to origin-form and force `Connection: close` so
/// upstream EOF-delimits the response. Also pins Accept-Encoding to gzip so
/// captured bodies can always be decoded for display.
fn rewrite_request_head(head: &str) -> String {
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");
    let ver = parts.next().unwrap_or("HTTP/1.1");
    let path = if target.starts_with("http://") || target.starts_with("https://") {
        split_url(target).1
    } else {
        target.to_string()
    };
    let mut out = format!("{method} {path} {ver}\r\n");
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (key, _value) = match line.split_once(':') {
            Some(kv) => kv,
            None => continue,
        };
        let name = key.trim();
        if name.eq_ignore_ascii_case("connection")
            || name.eq_ignore_ascii_case("accept-encoding")
            || name.starts_with("proxy-")
        {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("Accept-Encoding: gzip\r\n");
    out.push_str("Connection: close\r\n\r\n");
    out
}

/// Force the downstream response to close so the TV reconnects per request.
fn rewrite_response_head(head: &str) -> String {
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default().to_string();
    let mut out = format!("{first}\r\n");
    for line in lines {
        if line.is_empty() {
            break;
        }
        if line
            .split_once(':')
            .map(|(k, _)| k.trim().eq_ignore_ascii_case("connection"))
            .unwrap_or(false)
        {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    out
}

/// Serve one decrypted inner HTTPS request: capture it, forward to the real
/// origin over TLS, capture and forward the response. Returns Err to end the
/// tunnel loop.
async fn serve_https_request<S>(
    client: &mut S,
    host: &str,
    raw: Vec<u8>,
    log: &Arc<NetworkLog>,
    on_entry: &Arc<dyn Fn(NetworkEntry) + Send + Sync>,
) -> Result<()>
where
    S: AsyncWrite + AsyncRead + Unpin,
{
    let started = chrono::Utc::now().timestamp_millis() as u64;
    let head_end = find_head_end(&raw).unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let first = head.lines().next().unwrap_or_default().to_string();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let path = if target.starts_with('/') {
        target.clone()
    } else {
        format!("/{target}")
    };

    let request_headers = headers_from_head(&head);
    let request_body = body_preview(&raw[head_end + 4..]);

    let mut entry = NetworkEntry {
        id: log.next_id(),
        started_at: started,
        method,
        url: entry_url(true, host, &path),
        host: host.to_string(),
        path: path.clone(),
        status: None,
        duration_ms: None,
        size: None,
        encrypted: true,
        tls_error: None,
        request_headers: request_headers.clone(),
        response_headers: Default::default(),
        request_body,
        response_body: None,
    };
    on_entry(log.push(entry.clone()));

    // Connect to the origin over normal TLS.
    let connector = TlsConnector::from(Arc::new(crate::mitm_ca::upstream_client_config().clone()));
    let server_name = match ServerName::try_from(host.to_string()) {
        Ok(name) => name,
        Err(e) => {
            respond_error(client, &mut entry, 502, format!("invalid hostname: {e}"), on_entry);
            return Ok(());
        }
    };
    let tcp = match TcpStream::connect((host, 443)).await {
        Ok(t) => t,
        Err(e) => {
            respond_error(client, &mut entry, 502, format!("TCP connect failed: {e}"), on_entry);
            return Ok(());
        }
    };
    let mut upstream = match connector.connect(server_name, tcp).await {
        Ok(u) => u,
        Err(e) => {
            let hint = if e.to_string().contains("certificate") {
                " (origin certificate not trusted — pinned API?)".to_string()
            } else {
                String::new()
            };
            respond_error(
                client,
                &mut entry,
                502,
                format!("upstream TLS failed: {e}{hint}"),
                on_entry,
            );
            return Ok(());
        }
    };

    upstream.write_all(rewrite_request_head(&head).as_bytes()).await?;
    upstream.write_all(&raw[head_end + 4..]).await?;

    let mut resp = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        upstream.read_to_end(&mut resp),
    )
    .await;
    if resp.is_empty() {
        respond_error(
            client,
            &mut entry,
            504,
            "no response from origin within 30s".into(),
            on_entry,
        );
        return Ok(());
    }

    // Forward the raw response bytes to the TV unchanged (with Connection: close).
    let resp_head_end = find_head_end(&resp).unwrap_or(resp.len());
    let resp_head = String::from_utf8_lossy(&resp[..resp_head_end]).to_string();
    let rewritten = rewrite_response_head(&resp_head);
    let mut out = rewritten.into_bytes();
    out.extend_from_slice(&resp[resp_head_end + 4..]);
    client.write_all(&out).await?;
    client.flush().await?;

    entry.status = parse_status(&resp);
    let (response_headers, _) = parse_headers_and_body(&resp);
    let display_body = display_response_body(&resp_head, &resp[resp_head_end + 4..]);
    entry.response_headers = response_headers;
    entry.response_body = display_body;
    entry.size = Some(resp.len() as u64);
    entry.duration_ms =
        Some((chrono::Utc::now().timestamp_millis() as u64).saturating_sub(started));
    on_entry(log.push(entry));
    Ok(())
}

/// Reply with a synthetic error response (both to the TV and into the log).
fn respond_error<S>(
    client: &mut S,
    entry: &mut NetworkEntry,
    status: u16,
    message: String,
    on_entry: &Arc<dyn Fn(NetworkEntry) + Send + Sync>,
) where
    S: AsyncWrite + Unpin,
{
    let body = format!("{{\"mitmError\":{}}}", serde_json::json!(message));
    let head = format!(
        "HTTP/1.1 {status} Mitm Error\r\nContent-Type: application/json\r\nX-Mitm-Error: {}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        message.replace('\r', " ").replace('\n', " "),
        body.len()
    );
    let _ = client.write_all(head.as_bytes());
    let _ = client.write_all(body.as_bytes());
    let _ = client.flush();
    entry.status = Some(status);
    entry.response_headers.insert("X-Mitm-Error".into(), message);
    entry.response_body = Some(body);
    entry.duration_ms = Some(
        (chrono::Utc::now().timestamp_millis() as u64).saturating_sub(entry.started_at),
    );
    on_entry(entry.clone());
}

/// Record a failed TLS handshake against the TV as an explicit error entry.
/// System hosts are never recorded, and repeat failures of the same host are
/// console-only so the Network tab stays usable.
fn record_tls_failure(
    host: &str,
    err: impl std::fmt::Display,
    log: &Arc<NetworkLog>,
    on_entry: &Arc<dyn Fn(NetworkEntry) + Send + Sync>,
    started: u64,
) {
    let msg = err.to_string();
    let detail = if msg.contains("CertificateUnknown")
        || msg.contains("certificate_unknown")
        || msg.contains("alert 46")
    {
        format!(
            "{msg} — the TV rejected our MITM certificate. If this repeats, the app is pinning or our CA is untrusted."
        )
    } else {
        msg
    };
    println!("[mitm] TLS handshake failed for {host}: {detail}");
    if is_system_host(host) {
        return; // OS/Google noise — never shown in the UI
    }
    if !log.should_record_tls_failure(host) {
        return; // already recorded once for this host
    }
    let entry = NetworkEntry {
        id: log.next_id(),
        started_at: started,
        method: "CONNECT".into(),
        url: entry_url(true, host, ""),
        host: host.to_string(),
        path: String::new(),
        status: None,
        duration_ms: Some((chrono::Utc::now().timestamp_millis() as u64).saturating_sub(started)),
        size: None,
        encrypted: true,
        tls_error: Some(detail),
        request_headers: Default::default(),
        response_headers: Default::default(),
        request_body: None,
        response_body: None,
    };
    on_entry(log.push(entry));
}

fn headers_from_head(head: &str) -> std::collections::HashMap<String, String> {
    let mut headers = std::collections::HashMap::new();
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_string(), value.trim().to_string());
    }
    headers
}

/// Decode chunked transfer coding.
fn dechunk(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() && out.len() < MAX_HTTP_MESSAGE {
        let line_end = match data[i..].windows(2).position(|w| w == b"\r\n") {
            Some(p) => i + p,
            None => break,
        };
        let line = String::from_utf8_lossy(&data[i..line_end]);
        let size = match usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16) {
            Ok(s) => s,
            Err(_) => break,
        };
        i = line_end + 2;
        if size == 0 {
            break;
        }
        let end = (i + size).min(data.len());
        out.extend_from_slice(&data[i..end]);
        i = end + 2.min(data.len() - end);
    }
    out
}

/// Decompress gzip/deflate content encodings so bodies are readable.
fn decode_content_encoding(bytes: Vec<u8>, encoding: Option<&str>) -> Vec<u8> {
    let enc = encoding.map(|e| e.trim().to_ascii_lowercase());
    match enc.as_deref() {
        Some("gzip" | "x-gzip") => {
            let mut out = Vec::new();
            if GzDecoder::new(&bytes[..]).read_to_end(&mut out).is_ok() && !out.is_empty() {
                return out;
            }
        }
        Some("deflate") => {
            let mut out = Vec::new();
            if ZlibDecoder::new(&bytes[..]).read_to_end(&mut out).is_ok() && !out.is_empty() {
                return out;
            }
            let mut out = Vec::new();
            if DeflateDecoder::new(&bytes[..]).read_to_end(&mut out).is_ok() && !out.is_empty() {
                return out;
            }
        }
        _ => {}
    }
    bytes
}

/// Build the human-readable response body shown in the Network tab:
/// dechunked, decompressed, JSON pretty-printed, binary shown as hex.
fn display_response_body(resp_head: &str, body: &[u8]) -> Option<String> {
    let headers = headers_from_head(resp_head);
    let chunked = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("transfer-encoding"))
        .map(|(_, v)| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    let bytes = if chunked { dechunk(body) } else { body.to_vec() };
    let encoding = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, v)| v.as_str());
    body_preview(&decode_content_encoding(bytes, encoding))
}

const MAX_BODY_BYTES: usize = 256 * 1024;

/// Convert raw body bytes to display text: UTF-8 text (JSON pretty-printed)
/// or a binary placeholder with hex dump. Capped at MAX_BODY_BYTES.
fn body_preview(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let truncated_bytes = body.len() > MAX_BODY_BYTES;
    let slice = if truncated_bytes { &body[..MAX_BODY_BYTES] } else { body };

    if looks_binary(slice) {
        let hex: String = slice
            .iter()
            .take(256)
            .map(|b| format!("{b:02x} "))
            .collect();
        return Some(format!(
            "[binary payload — {} bytes{}]\nhex prefix: {}",
            body.len(),
            if truncated_bytes { ", truncated" } else { "" },
            hex.trim_end()
        ));
    }

    let text = String::from_utf8_lossy(slice);
    let trimmed = text.trim_end_matches('\0').trim();
    if trimmed.is_empty() {
        return None;
    }

    // Pretty-print JSON when possible.
    let rendered = serde_json::from_str::<serde_json::Value>(trimmed)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or_else(|_| trimmed.to_string());

    const MAX_CHARS: usize = 64_000;
    let count = rendered.chars().count();
    if count <= MAX_CHARS && !truncated_bytes {
        return Some(rendered);
    }
    let mut out: String = rendered.chars().take(MAX_CHARS).collect();
    out.push_str(&format!(
        "\n… truncated (showing {} of {} bytes)",
        slice.len(),
        body.len()
    ));
    Some(out)
}

fn looks_binary(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let sample = &data[..data.len().min(2048)];
    let controls = sample
        .iter()
        .filter(|b| **b < 9 || (**b > 13 && **b < 32 && **b != 27))
        .count();
    controls * 10 > sample.len()
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
    use crate::mitm_ca::MitmCa;

    /// End-to-end MITM smoke test: CONNECT through the proxy, speak TLS to
    /// the fake leaf, GET /, and confirm the entry captured the full request.
    #[tokio::test]
    async fn mitm_captures_inner_https_request() -> std::io::Result<()> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        // Reachability guard — skip gracefully without internet.
        if tokio::net::TcpStream::connect("example.com:443").await.is_err() {
            return Ok(());
        }

        let ca = MitmCa::resolve(&[]).map_err(std::io::Error::other)?;
        let log = Arc::new(NetworkLog::new());
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let log_for_cb = log.clone();
        let log_for_task = log.clone();
        let ca_for_task = ca.clone();
        let on_entry = move |e: NetworkEntry| {
            log_for_cb.push(e);
        };
        tokio::spawn(async move {
            let _ = start_proxy_inner_test(port, log_for_task, ca_for_task, on_entry).await;
        });

        // Client side of the tunnel, trusting our CA root directly.
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                ca_ca_der(&ca).clone(),
            ))
            .unwrap();
        let config = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let mut tcp = None;
        for _ in 0..50 {
            match TcpStream::connect(("127.0.0.1", port)).await {
                Ok(s) => {
                    tcp = Some(s);
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }
        let mut tcp = tcp.expect("proxy never became ready");

        // Standard HTTP proxy flow: plain CONNECT first, then TLS upgrade.
        tcp.write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await?;
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            let n = tcp.read(&mut byte).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&byte);
        }
        assert!(
            String::from_utf8_lossy(&buf).contains("200"),
            "CONNECT not accepted: {}",
            String::from_utf8_lossy(&buf)
        );
        let mut client = TlsConnector::from(config)
            .connect("example.com".try_into().unwrap(), tcp)
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        client
            .write_all(b"GET /?q=tv HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
            .await?;
        let mut resp = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client.read_to_end(&mut resp),
        )
        .await;
        assert!(!resp.is_empty(), "no response through MITM");
        let head = String::from_utf8_lossy(&resp[..resp.len().min(2048)]).to_string();
        assert!(head.starts_with("HTTP/1."), "bad response head: {head}");

        // The inner request must be in the log with full URL + headers.
        let entries = log.list();
        let hit = entries
            .iter()
            .find(|e| e.method == "GET" && e.url == "https://example.com/?q=tv")
            .expect("inner HTTPS request not captured");
        assert!(hit.request_headers.contains_key("Host"));
        assert!(hit.status.unwrap_or(0) >= 200);

        // And the CONNECT row exists as the parent.
        assert!(entries.iter().any(|e| e.method == "CONNECT" && e.host == "example.com"));
        Ok(())
    }

    async fn start_proxy_inner_test(
        port: u16,
        log: Arc<NetworkLog>,
        ca: Arc<MitmCa>,
        on_entry: impl Fn(NetworkEntry) + Send + Sync + 'static,
    ) -> Result<()> {
        start_proxy(port, log, ca, on_entry).await.map(|_| ())
    }

    fn ca_ca_der(ca: &Arc<MitmCa>) -> &Vec<u8> {
        // Access via a tiny helper so the field stays private otherwise.
        ca.ca_cert_der_for_test()
    }

    #[test]
    fn system_hosts_are_detected_by_suffix() {
        assert!(is_system_host("connectivitycheck.gstatic.com"));
        assert!(is_system_host("www.google.com."));
        assert!(is_system_host("android.googleapis.com"));
        assert!(!is_system_host("api.jawwy.tv"));
        assert!(!is_system_host("notgoogle.com"));
    }

    #[test]
    fn tls_failure_recorded_once_per_host() {
        let log = NetworkLog::new();
        assert!(log.should_record_tls_failure("pinned.tv"));
        assert!(!log.should_record_tls_failure("pinned.tv"));
        assert!(log.should_record_tls_failure("other.tv"));
    }

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

    #[test]
    fn rewrites_absolute_head_and_forces_close() {
        let head = "POST http://api.tv/v1/x HTTP/1.1\r\nHost: api.tv\r\nConnection: keep-alive\r\nContent-Length: 2\r\n\r\n";
        let out = rewrite_request_head(head);
        assert!(out.starts_with("POST /v1/x HTTP/1.1\r\n"));
        assert!(out.contains("Content-Length: 2\r\n"));
        assert!(!out.contains("keep-alive"));
        assert!(out.ends_with("Connection: close\r\n\r\n"));
    }

    #[test]
    fn finds_head_end_and_content_length() {
        let data = b"GET / HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\n\r\nhello";
        let end = find_head_end(data).unwrap();
        assert_eq!(&data[end + 4..], b"hello");
        assert_eq!(
            header_value(&String::from_utf8_lossy(&data[..end]), "content-length"),
            Some("5")
        );
    }
}
