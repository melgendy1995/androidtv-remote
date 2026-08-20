use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::tungstenite::Message;

use crate::adb::AdbClient;
use crate::error::{AppError, Result};
use crate::recorder::{build_avcc, contains_idr, split_sps_pps, to_avcc, VideoRecorder};

const CODEC_H264: u32 = u32::from_be_bytes(*b"h264");
const PACKET_FLAG_CONFIG: u64 = 1 << 63;
const PACKET_FLAG_KEY_FRAME: u64 = 1 << 62;
const MAX_PACKET_SIZE: usize = 64 * 1024 * 1024;

pub const SCRCPY_VERSION: &str = "3.3.1";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStatus {
    pub streaming: bool,
    pub video_port: Option<u16>,
    pub width: u32,
    pub height: u32,
    pub last_frame_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct FramePacket {
    pub flags: u8,
    pub payload: Vec<u8>,
}

pub struct StreamConfig {
    pub max_size: u32,
    pub video_bit_rate: u32,
    pub max_fps: u32,
    pub audio: bool,
}

struct StreamRuntime {
    adb: AdbClient,
    serial: String,
    scid: String,
    video_listener: TcpListener,
    websocket_listener: TcpListener,
    stop: tokio::sync::watch::Receiver<bool>,
    config: StreamConfig,
}

pub struct ScrcpySession {
    pub status: Mutex<StreamStatus>,
    pub recorder: Arc<VideoRecorder>,
    frames: broadcast::Sender<FramePacket>,
    stop: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    decoder_config: Mutex<Option<Vec<u8>>>,
    latest_keyframe: Mutex<Option<Vec<u8>>>,
    sps: Mutex<Vec<u8>>,
    pps: Mutex<Vec<u8>>,
    tunnel: Mutex<Option<(AdbClient, String, String)>>,
}

impl ScrcpySession {
    pub fn new() -> Self {
        let (frames, _) = broadcast::channel(256);
        Self {
            status: Mutex::new(StreamStatus::default()),
            recorder: Arc::new(VideoRecorder::new()),
            frames,
            stop: Mutex::new(None),
            decoder_config: Mutex::new(None),
            latest_keyframe: Mutex::new(None),
            sps: Mutex::new(Vec::new()),
            pps: Mutex::new(Vec::new()),
            tunnel: Mutex::new(None),
        }
    }

    pub async fn snapshot(&self) -> StreamStatus {
        self.status.lock().await.clone()
    }

    pub async fn sps_snapshot(&self) -> Vec<u8> {
        self.sps.lock().await.clone()
    }

    pub async fn pps_snapshot(&self) -> Vec<u8> {
        self.pps.lock().await.clone()
    }

    pub async fn stop(&self) {
        if let Some(tx) = self.stop.lock().await.take() {
            let _ = tx.send(true);
        }
        let _ = self.recorder.finish();
        if let Some((adb, serial, scid)) = self.tunnel.lock().await.take() {
            let remote = format!("localabstract:scrcpy_{scid}");
            let _ = adb
                .run_serial(Some(&serial), &["reverse", "--remove", &remote])
                .await;
            let _ = adb
                .shell(&serial, "pkill -f com.genymobile.scrcpy.Server")
                .await;
        }
        let mut status = self.status.lock().await;
        status.streaming = false;
        status.video_port = None;
    }

    pub async fn start(
        self: Arc<Self>,
        adb: AdbClient,
        serial: String,
        resource_dir: PathBuf,
        config: StreamConfig,
    ) -> Result<StreamStatus> {
        self.stop().await;
        if config.audio {
            return Err(AppError::from(
                "Audio streaming is not implemented yet; disable audio in Settings.",
            ));
        }
        *self.decoder_config.lock().await = None;
        *self.latest_keyframe.lock().await = None;
        *self.sps.lock().await = Vec::new();
        *self.pps.lock().await = Vec::new();
        let server_jar = find_server_jar(&resource_dir)?;
        adb.push(&serial, &server_jar, "/data/local/tmp/scrcpy-server.jar")
            .await?;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let video_listen = listener.local_addr()?.port();
        let scid = format!("{:08x}", rand::random::<u32>() & 0x7fff_ffff);
        let remote = format!("localabstract:scrcpy_{scid}");
        let local = format!("tcp:{video_listen}");
        adb.reverse(&serial, &remote, &local).await?;
        *self.tunnel.lock().await = Some((adb.clone(), serial.clone(), scid.clone()));

        let ws_listener = TcpListener::bind("127.0.0.1:0").await?;
        let ws_port = ws_listener.local_addr()?.port();

        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        *self.stop.lock().await = Some(stop_tx);

        {
            let mut status = self.status.lock().await;
            status.streaming = true;
            status.video_port = Some(ws_port);
            status.error = None;
            status.last_frame_at = None;
        }

        let session = self.clone();
        let runtime = StreamRuntime {
            adb,
            serial,
            scid,
            video_listener: listener,
            websocket_listener: ws_listener,
            stop: stop_rx,
            config,
        };
        tokio::spawn(async move {
            if let Err(error) = run_server(session.clone(), runtime).await {
                mark_stopped(&session, Some(error.to_string())).await;
            }
        });

        Ok(self.snapshot().await)
    }
}

async fn run_server(
    session: Arc<ScrcpySession>,
    runtime: StreamRuntime,
) -> Result<()> {
    let StreamRuntime {
        adb,
        serial,
        scid,
        video_listener: listener,
        websocket_listener: ws_listener,
        mut stop,
        config,
    } = runtime;

    tokio::spawn(ws_loop(ws_listener, session.clone(), stop.clone()));

    let audio_str = if config.audio { "true" } else { "false" };
    let mut cmd = format!(
        "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server {SCRCPY_VERSION} scid={scid} log_level=info audio={audio_str} control=false stay_awake=true video_codec=h264"
    );
    if config.max_size > 0 {
        cmd.push_str(&format!(" max_size={}", config.max_size));
    }
    if config.video_bit_rate > 0 {
        cmd.push_str(&format!(" video_bit_rate={}", config.video_bit_rate));
    }
    if config.max_fps > 0 {
        cmd.push_str(&format!(" max_fps={}", config.max_fps));
    }
    let mut child = match adb.spawn_shell(&serial, &cmd) {
        Ok(child) => child,
        Err(e) => {
            mark_stopped(&session, Some(e.to_string())).await;
            return Ok(());
        }
    };
    drain_child_pipes(&mut child);

    let accept = tokio::time::timeout(Duration::from_secs(8), listener.accept());
    tokio::pin!(accept);
    let (mut video, _) = tokio::select! {
        _ = stop.changed() => {
            let _ = child.kill().await;
            mark_stopped(&session, None).await;
            return Ok(());
        }
        res = &mut accept => {
            match res {
                Ok(Ok(pair)) => pair,
                Ok(Err(e)) => {
                    let _ = child.kill().await;
                    mark_stopped(&session, Some(e.to_string())).await;
                    return Ok(());
                }
                Err(_) => {
                    let _ = child.kill().await;
                    mark_stopped(&session, Some("scrcpy video socket timed out".into())).await;
                    return Ok(());
                }
            }
        }
    };

    eprintln!("[stream] scrcpy socket accepted for {serial}");
    let mut name = [0u8; 64];
    tokio::time::timeout(Duration::from_secs(5), video.read_exact(&mut name))
        .await
        .map_err(|_| AppError::from("scrcpy device metadata timed out"))??;

    let metadata = read_video_metadata(&mut video).await?;
    eprintln!(
        "[stream] codec=h264 size={}x{} device={}",
        metadata.width,
        metadata.height,
        String::from_utf8_lossy(&name).trim_end_matches('\0')
    );
    {
        let mut status = session.status.lock().await;
        status.width = metadata.width;
        status.height = metadata.height;
    }

    let _ = video.set_nodelay(true);

    // Never cancel an in-flight packet read, and do not treat a quiet encoder as
    // dead: many Android TV chipsets emit no frames while the screen is static.
    let mut encoder_alive = false;
    let mut logged_first_frame = false;
    let startup = tokio::time::sleep(Duration::from_secs(20));
    tokio::pin!(startup);

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    let _ = child.kill().await;
                    mark_stopped(&session, None).await;
                    break;
                }
            }
            _ = &mut startup, if !encoder_alive => {
                let _ = child.kill().await;
                mark_stopped(&session, Some("no video frame in 20s".into())).await;
                break;
            }
            packet = read_header(&mut video) => {
                match packet {
                    Ok(header) => {
                        encoder_alive = true;
                        ingest_media_packet(&session, header, &mut logged_first_frame).await;
                    }
                    Err(e) => {
                        eprintln!("[stream] packet read failed: {e}");
                        let _ = child.kill().await;
                        mark_stopped(&session, Some(e.to_string())).await;
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct VideoMetadata {
    codec_id: u32,
    width: u32,
    height: u32,
}

enum Packet {
    Media {
        config: bool,
        key: bool,
        payload: Vec<u8>,
    },
}

async fn read_video_metadata(stream: &mut TcpStream) -> Result<VideoMetadata> {
    let mut raw = [0u8; 12];
    stream.read_exact(&mut raw).await?;
    parse_video_metadata(raw)
}

fn parse_video_metadata(raw: [u8; 12]) -> Result<VideoMetadata> {
    let metadata = VideoMetadata {
        codec_id: u32::from_be_bytes(raw[0..4].try_into().unwrap()),
        width: u32::from_be_bytes(raw[4..8].try_into().unwrap()),
        height: u32::from_be_bytes(raw[8..12].try_into().unwrap()),
    };
    if metadata.codec_id != CODEC_H264 {
        return Err(AppError::from(format!(
            "unsupported scrcpy video codec 0x{:08x}; select H.264",
            metadata.codec_id
        )));
    }
    if metadata.width == 0 || metadata.height == 0 {
        return Err(AppError::from("scrcpy returned an invalid video size"));
    }
    Ok(metadata)
}

async fn read_header(stream: &mut TcpStream) -> Result<Packet> {
    let mut hdr = [0u8; 12];
    stream.read_exact(&mut hdr).await?;
    let (config, key, size) = parse_frame_header(hdr)?;
    let mut payload = vec![0u8; size];
    stream.read_exact(&mut payload).await?;
    Ok(Packet::Media {
        config,
        key,
        payload,
    })
}

fn parse_frame_header(hdr: [u8; 12]) -> Result<(bool, bool, usize)> {
    let pts_flags = u64::from_be_bytes(hdr[0..8].try_into().unwrap());
    let size = u32::from_be_bytes(hdr[8..12].try_into().unwrap()) as usize;
    if size == 0 || size > MAX_PACKET_SIZE {
        return Err(AppError::from(format!("invalid scrcpy packet size {size}")));
    }
    Ok((
        pts_flags & PACKET_FLAG_CONFIG != 0,
        pts_flags & PACKET_FLAG_KEY_FRAME != 0,
        size,
    ))
}

async fn ws_loop(
    listener: TcpListener,
    session: Arc<ScrcpySession>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() { break; }
            }
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { break; };
                let rx = session.frames.subscribe();
                let stop = stop.clone();
                tokio::spawn(client_loop(stream, rx, session.clone(), stop));
            }
        }
    }
}

async fn client_loop(
    stream: TcpStream,
    mut rx: broadcast::Receiver<FramePacket>,
    session: Arc<ScrcpySession>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (mut sink, mut incoming) = ws.split();
    if !send_decoder_bootstrap(&mut sink, &session).await {
        return;
    }
    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() { break; }
            }
            msg = incoming.next() => {
                if msg.is_none() { break; }
            }
            pkt = rx.recv() => {
                match pkt {
                    Ok(frame) => {
                        if sink
                            .send(Message::Binary(flagged_bytes(frame.flags, &frame.payload).into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        eprintln!("[stream] ws client lagged, skipped {skipped} frames");
                        if !send_decoder_bootstrap(&mut sink, &session).await {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

async fn ingest_media_packet(
    session: &ScrcpySession,
    packet: Packet,
    logged_first_frame: &mut bool,
) {
    let Packet::Media {
        config,
        key,
        payload,
    } = packet;
    if config {
        store_decoder_config(session, payload).await;
        return;
    }
    publish_video_sample(session, payload, key, logged_first_frame).await;
}

async fn store_decoder_config(session: &ScrcpySession, payload: Vec<u8>) {
    let (sps, pps) = split_sps_pps(&payload);
    eprintln!(
        "[stream] config packet={} bytes sps={} pps={}",
        payload.len(),
        sps.len(),
        pps.len()
    );
    *session.sps.lock().await = sps.clone();
    *session.pps.lock().await = pps.clone();
    let (width, height) = {
        let status = session.status.lock().await;
        (status.width, status.height)
    };
    session
        .recorder
        .set_config(sps.clone(), pps.clone(), width, height);
    let avcc = build_avcc(&sps, &pps);
    *session.decoder_config.lock().await = Some(avcc.clone());
    let _ = session.frames.send(FramePacket {
        flags: 0x01,
        payload: avcc,
    });
}

async fn publish_video_sample(
    session: &ScrcpySession,
    payload: Vec<u8>,
    key: bool,
    logged_first_frame: &mut bool,
) {
    let key = key || contains_idr(&payload);
    if !*logged_first_frame {
        eprintln!("[stream] first frame={} bytes key={key}", payload.len());
        *logged_first_frame = true;
    }
    {
        let mut status = session.status.lock().await;
        status.last_frame_at = Some(unix_now_ms());
    }
    let avcc = to_avcc(&payload);
    if key {
        *session.latest_keyframe.lock().await = Some(avcc.clone());
    }
    if session.recorder.is_recording() {
        let _ = session.recorder.push_sample(&avcc, key);
    }
    let _ = session.frames.send(FramePacket {
        flags: if key { 0x02 } else { 0 },
        payload: avcc,
    });
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn drain_child_pipes(child: &mut tokio::process::Child) {
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(forward_pipe_logs(stdout, "scrcpy"));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(forward_pipe_logs(stderr, "scrcpy"));
    }
}

async fn forward_pipe_logs<R>(mut pipe: R, prefix: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let mut buf = [0u8; 4096];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                for line in String::from_utf8_lossy(&buf[..n]).lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        eprintln!("[{prefix}] {line}");
                    }
                }
            }
        }
    }
}

fn flagged_bytes(flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1 + payload.len());
    bytes.push(flags);
    bytes.extend_from_slice(payload);
    bytes
}

async fn decoder_bootstrap_packets(session: &ScrcpySession) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    if let Some(config) = session.decoder_config.lock().await.clone() {
        packets.push(flagged_bytes(0x01, &config));
    }
    if let Some(keyframe) = session.latest_keyframe.lock().await.clone() {
        packets.push(flagged_bytes(0x02, &keyframe));
    }
    packets
}

async fn send_decoder_bootstrap<S>(sink: &mut S, session: &ScrcpySession) -> bool
where
    S: futures_util::Sink<Message> + Unpin,
{
    for packet in decoder_bootstrap_packets(session).await {
        if sink.send(Message::Binary(packet.into())).await.is_err() {
            return false;
        }
    }
    true
}

async fn mark_stopped(session: &ScrcpySession, error: Option<String>) {
    let mut status = session.status.lock().await;
    status.streaming = false;
    status.error = error;
}

fn find_server_jar(resource_dir: &Path) -> Result<PathBuf> {
    let candidates = [
        resource_dir.join("resources").join("scrcpy-server"),
        resource_dir.join("scrcpy-server"),
        PathBuf::from("resources/scrcpy-server"),
        PathBuf::from("src-tauri/resources/scrcpy-server"),
        PathBuf::from("../src-tauri/resources/scrcpy-server"),
    ];
    for path in candidates {
        if path.exists() && path.is_file() {
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.len() > 10000 {
                    if let Ok(abs) = path.canonicalize() {
                        return Ok(crate::adb::strip_verbatim_prefix(&abs));
                    }
                    return Ok(path);
                }
            }
        }
    }
    Err(AppError::from(
        "scrcpy-server is missing. Run npm run sidecars.",
    ))
}

pub fn parse_now_playing(dumpsys: &str) -> (String, String, String) {
    let mut title = String::new();
    let mut artist = String::new();
    let mut package = String::new();
    for line in dumpsys.lines() {
        let line = line.trim();
        if package.is_empty() {
            if let Some(rest) = line.strip_prefix("package=") {
                package = rest.split_whitespace().next().unwrap_or("").to_string();
            } else if line.contains("packageName=") {
                if let Some(rest) = line.split("packageName=").nth(1) {
                    package = rest.split([',', ' ']).next().unwrap_or("").to_string();
                }
            }
        }
        if title.is_empty() && line.contains("metadata: title=") {
            title = line
                .split("title=")
                .nth(1)
                .unwrap_or("")
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string();
        }
        if artist.is_empty() && (line.contains("artist=") || line.contains("subtitle=")) {
            artist = line
                .split(['='])
                .nth(1)
                .unwrap_or("")
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string();
        }
    }
    (package, title, artist)
}

pub fn keyboard_focused(dump: &str) -> bool {
    ime_service_visible(dump) || editable_field_active(dump) || onscreen_keyboard_present(dump)
}

fn ime_service_visible(dump: &str) -> bool {
    let mut in_ime_window = false;
    for line in dump.lines() {
        let lower = line.trim().to_ascii_lowercase();
        if dump_flag(&lower, "minputshown") || dump_flag(&lower, "misinputviewshown") {
            return true;
        }
        if ime_visibility_shown(&lower) {
            return true;
        }
        if lower.contains("mcurrentfocus") && lower.contains("inputmethod") {
            return true;
        }
        let window_header = (lower.starts_with("window #") || lower.starts_with("window{"))
            && lower.contains("inputmethod");
        if window_header {
            in_ime_window = true;
            continue;
        }
        if in_ime_window {
            if lower.starts_with("window #") || lower.starts_with("window{") {
                in_ime_window = false;
            } else if dump_flag(&lower, "isvisible") {
                return true;
            }
        }
    }
    false
}

fn editable_field_active(dump: &str) -> bool {
    dump.split("<node").any(|chunk| node_is_editable(chunk) && node_flag(chunk, "focused"))
}

fn onscreen_keyboard_present(dump: &str) -> bool {
    dump.split("<node").any(|chunk| {
        let class = xml_attr(chunk, "class")
            .unwrap_or_default()
            .to_ascii_lowercase();
        class.contains("keyboardview")
            || class.contains("inputmethodservice.keyboard")
            || class.contains("leanbackime")
    })
}

fn node_is_editable(node: &str) -> bool {
    let class = xml_attr(node, "class")
        .unwrap_or_default()
        .to_ascii_lowercase();
    class.contains("edit")
        || class.contains("autocomplete")
        || class.contains("searchbar")
        || class.contains("searchview")
        || class.contains("textinput")
        || class.contains("inputfield")
        || class.contains("textfield")
        || node_flag(node, "password")
}

fn node_flag(node: &str, name: &str) -> bool {
    xml_attr(node, name).is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub fn editor_text(dump: &str) -> String {
    if let Some(text) = focused_uiautomator_text(dump) {
        return text;
    }
    extracted_editor_text(dump).unwrap_or_default()
}

fn extracted_editor_text(dump: &str) -> Option<String> {
    let lower = dump.to_ascii_lowercase();
    let key = "extractedtext{text=";
    let idx = lower.find(key)?;
    let rest = &dump[idx + key.len()..];
    let end = rest
        .find(", startOffset")
        .or_else(|| rest.find(", startoffset"))
        .or_else(|| rest.find('}'))
        .unwrap_or(rest.len());
    let text = rest[..end].trim().trim_matches('"');
    if text.is_empty() || text.eq_ignore_ascii_case("null") {
        None
    } else {
        Some(text.to_string())
    }
}

fn focused_uiautomator_text(xml: &str) -> Option<String> {
    let mut fallback = None;
    for chunk in xml.split("<node") {
        let lower = chunk.to_ascii_lowercase();
        if !lower.contains("focused=\"true\"") {
            continue;
        }
        let text = xml_attr(chunk, "text").unwrap_or("");
        if text.is_empty() {
            continue;
        }
        let decoded = decode_xml_entities(text);
        let class = xml_attr(chunk, "class").unwrap_or("");
        let class_l = class.to_ascii_lowercase();
        if class_l.contains("edit") || class_l.contains("autocomplete") {
            return Some(decoded);
        }
        if fallback.is_none() {
            fallback = Some(decoded);
        }
    }
    fallback
}

fn xml_attr<'a>(node: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = node.find(&needle)? + needle.len();
    let rest = &node[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn ime_visibility_shown(line: &str) -> bool {
    let Some(value) = dump_value(line, "mimewindowvis") else {
        return false;
    };
    if value.contains("visible") {
        return true;
    }
    ime_visible_bits(value)
}

fn ime_visible_bits(raw: &str) -> bool {
    let token = raw.split('|').next().unwrap_or(raw).trim();
    let bits = if let Some(hex) = token.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        token.parse().ok()
    };
    bits.is_some_and(|value| value & 0x2 != 0)
}

fn dump_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=");
    let mut from = 0;
    while from < line.len() {
        let Some(rel) = line[from..].find(&needle) else {
            return None;
        };
        let idx = from + rel;
        let at_token_start = idx == 0 || !line.as_bytes()[idx - 1].is_ascii_alphanumeric();
        if at_token_start {
            let rest = &line[idx + needle.len()..];
            return Some(
                rest.split([',', ' ', '\t', '}'])
                    .next()
                    .unwrap_or(rest)
                    .trim(),
            );
        }
        from = idx + 1;
    }
    None
}

fn dump_flag(line: &str, key: &str) -> bool {
    dump_value(line, key).is_some_and(|value| value == "true")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcl_video_metadata() {
        let raw = [
            b'h', b'2', b'6', b'4', 0x00, 0x00, 0x07, 0x80, 0x00, 0x00, 0x04, 0x38,
        ];
        let metadata = parse_video_metadata(raw).unwrap();
        assert_eq!(metadata.codec_id, CODEC_H264);
        assert_eq!(metadata.width, 1920);
        assert_eq!(metadata.height, 1080);
    }

    #[test]
    fn parses_config_and_keyframe_flags_from_u64() {
        let mut config = [0u8; 12];
        config[0..8].copy_from_slice(&PACKET_FLAG_CONFIG.to_be_bytes());
        config[8..12].copy_from_slice(&39u32.to_be_bytes());
        assert_eq!(parse_frame_header(config).unwrap(), (true, false, 39));

        let mut key = [0u8; 12];
        key[0..8].copy_from_slice(&(PACKET_FLAG_KEY_FRAME | 1234).to_be_bytes());
        key[8..12].copy_from_slice(&61_851u32.to_be_bytes());
        assert_eq!(parse_frame_header(key).unwrap(), (false, true, 61_851));
    }

    #[test]
    fn rejects_unsupported_codec() {
        let raw = [
            b'h', b'2', b'6', b'5', 0x00, 0x00, 0x07, 0x80, 0x00, 0x00, 0x04, 0x38,
        ];
        assert!(parse_video_metadata(raw).is_err());
    }

    #[test]
    fn ignores_null_served_view_on_idle_client() {
        let dump = "\
Client #0:
  mServedView=null
  mInputShown=false
  mActive=false
";
        assert!(!keyboard_focused(dump));
    }

    #[test]
    fn leftover_served_view_without_ime_is_not_focus() {
        let dump = "\
Client #0:
  mServedView=null
  mInputShown=false
Client #1:
  mServedView=androidx.leanback.widget.SearchEditText{abc VFED..CL}
  mInputShown=false
";
        assert!(!keyboard_focused(dump));
    }

    #[test]
    fn detects_shown_ime_without_served_view() {
        assert!(keyboard_focused("mServedView=null\nmInputShown=true\n"));
    }

    #[test]
    fn catalog_input_types_and_sticky_ime_flags_are_not_focus() {
        assert!(!keyboard_focused(
            "mEditorInfo{inputType=0x1 packageName=com.netflix.mediaclient}\n"
        ));
        assert!(!keyboard_focused("mImeWindowVis=ACTIVE\nmServedView=null\n"));
        assert!(!keyboard_focused("mImeWindowVis=1\nmServedView=null\n"));
        assert!(!keyboard_focused(
            "mShowRequested=true\nmInputShown=false\nmServedView=null\n"
        ));
    }

    #[test]
    fn detects_visible_ime_from_aosp_and_numeric_dumps() {
        assert!(keyboard_focused("mImeWindowVis=ACTIVE|VISIBLE\nmServedView=null\n"));
        assert!(keyboard_focused("mImeWindowVis=IME_ACTIVE|IME_VISIBLE\n"));
        assert!(keyboard_focused("mImeWindowVis=3\nmServedView=null\n"));
        assert!(keyboard_focused("mImeWindowVis=2\n"));
        assert!(!keyboard_focused("mRequestedImeVisible=true\nmInputShown=false\n"));
        assert!(keyboard_focused(
            "Window{abc u0 InputMethod}:\n  mHasSurface=true\n  isVisible=true\n"
        ));
    }

    #[test]
    fn installed_ime_package_is_not_an_open_textbox() {
        let dump = "\
mCurMethodId=com.google.android.leanback.ime/.LeanbackImeService
InputMethodInfo{com.android.inputmethod.latin/.LatinIME}
mInputShown=false
mImeWindowVis=0
";
        assert!(!keyboard_focused(dump));
    }

    #[test]
    fn next_served_view_does_not_count_as_focus() {
        let dump = "mNextServedView=android.widget.EditText{abc}\nmServedView=null\n";
        assert!(!keyboard_focused(dump));
    }

    #[test]
    fn reads_extracted_and_focused_uiautomator_text() {
        assert_eq!(
            editor_text("ExtractedText{text=hello world, startOffset=0, selectionStart=11}"),
            "hello world"
        );
        let xml = r#"<node class="android.widget.TextView" focused="true" text="label"/><node class="android.widget.EditText" focused="true" text="netflix &amp; chill"/>"#;
        assert_eq!(editor_text(xml), "netflix & chill");
    }

    #[test]
    fn opens_on_focused_edit_text_not_selected_tile() {
        assert!(keyboard_focused(
            r#"<node class="androidx.leanback.widget.SearchEditText" focused="true" text=""/>"#
        ));
        assert!(!keyboard_focused(
            r#"<node class="android.widget.EditText" focused="false" selected="true" text="query"/>"#
        ));
        assert!(!keyboard_focused(
            r#"<node class="android.widget.Button" focused="true" text="OK"/>"#
        ));
    }

    #[test]
    fn opens_on_leanback_ime_and_inputmethod_focus_window() {
        assert!(keyboard_focused(
            "mCurrentFocus=Window{abc u0 InputMethod}\n"
        ));
        assert!(keyboard_focused(
            r#"<node class="com.google.android.leanback.ime.LeanbackImeService" package="com.google.android.leanback.ime"/>"#
        ));
    }
}
