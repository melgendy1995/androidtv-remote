mod adb;
mod apps;
mod clipboard;
mod crashes;
mod devices;
mod error;
mod files;
mod keys;
mod logcat;
mod paths;
mod proxy;
mod recorder;
mod scrcpy;
mod settings;

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

use adb::AdbClient;
use crashes::CrashLog;
use devices::{list_merged, ConnectionStatus, DeviceInfo, DeviceRegistry};
use error::{AppError, Result};
use keys::{command_to_keyevent, field_update_actions, InputAction};
use logcat::{
    extract_http, is_crash_line, is_stack_followup, spawn_logcat, LogLine, LogcatHub,
};
use proxy::{NetworkEntry, NetworkLog};
use scrcpy::{editor_text, keyboard_focused, parse_now_playing, ScrcpySession, StreamStatus};
use settings::Settings;

pub struct AppState {
    settings: Mutex<Settings>,
    adb: Mutex<AdbClient>,
    registry: Mutex<DeviceRegistry>,
    connected: Mutex<Option<DeviceInfo>>,
    scrcpy: Arc<ScrcpySession>,
    logcat: Arc<LogcatHub>,
    logcat_stop: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    log_buffer: Arc<Mutex<Vec<LogLine>>>,
    crashes: Arc<CrashLog>,
    network: Arc<NetworkLog>,
    resource_dir: std::path::PathBuf,
    proxy_port: Mutex<Option<u16>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub package_name: String,
    pub label: String,
    pub title: String,
    pub artist: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardState {
    pub focused: bool,
    pub text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording: bool,
    pub elapsed_ms: u64,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

fn rebuild_adb(settings: &Settings, resource_dir: &std::path::Path) -> AdbClient {
    AdbClient::new(settings, resource_dir)
}

async fn current_serial(state: &AppState) -> Result<String> {
    state
        .connected
        .lock()
        .await
        .as_ref()
        .map(|d| d.serial.clone())
        .ok_or_else(|| AppError::from("not connected"))
}

async fn emit_status(app: &AppHandle, state: &AppState) {
    if let Ok(status) = build_status(state).await {
        let _ = app.emit("status", status);
    }
}

async fn build_status(state: &AppState) -> Result<ConnectionStatus> {
    let adb = state.adb.lock().await.clone();
    let version = adb.version().await.ok();
    let connected = state.connected.lock().await.clone();
    let unauthorized = if let Some(dev) = &connected {
        adb.get_state(&dev.serial)
            .await
            .map(|s| s == "unauthorized")
            .unwrap_or(false)
    } else {
        false
    };
    Ok(ConnectionStatus {
        connected: connected.is_some(),
        serial: connected.as_ref().map(|d| d.serial.clone()),
        device: connected,
        unauthorized,
        adb_ok: version.is_some(),
        adb_version: version,
        adb_error: None,
    })
}

#[tauri::command]
async fn status(state: State<'_, Arc<AppState>>) -> Result<ConnectionStatus> {
    build_status(&state).await
}

#[tauri::command]
async fn list_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceInfo>> {
    let adb = state.adb.lock().await.clone();
    let registry = state.registry.lock().await;
    list_merged(&adb, &registry).await
}

#[tauri::command]
async fn connect_device(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    serial: String,
) -> Result<DeviceInfo> {
    connect_serial(&app, &state, serial).await
}

#[tauri::command]
async fn connect_host(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    host: String,
) -> Result<DeviceInfo> {
    let adb = state.adb.lock().await.clone();
    let serial = adb.connect_host(&host).await?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    connect_serial(&app, &state, serial).await
}

async fn connect_serial(app: &AppHandle, state: &AppState, serial: String) -> Result<DeviceInfo> {
    let adb = state.adb.lock().await.clone();
    let mut ready = adb.get_state(&serial).await.unwrap_or_else(|_| "offline".into());
    if ready == "offline" && serial.contains(':') {
        for _ in 0..3 {
            let _ = adb.connect_host(&serial).await;
            tokio::time::sleep(Duration::from_millis(600)).await;
            ready = adb.get_state(&serial).await.unwrap_or_else(|_| "offline".into());
            if ready == "device" || ready == "unauthorized" {
                break;
            }
        }
    }
    if ready == "unauthorized" {
        let device = DeviceInfo {
            serial: serial.clone(),
            name: serial.clone(),
            model: String::new(),
            android_version: String::new(),
            state: "unauthorized".into(),
            host: if serial.contains(':') {
                Some(serial.clone())
            } else {
                None
            },
            saved: false,
            last_connected_at: None,
        };
        *state.connected.lock().await = Some(device.clone());
        emit_status(app, state).await;
        return Ok(device);
    }
    if ready != "device" {
        return Err(AppError::from(format!(
            "device {serial} is {ready}. Enable USB/wireless debugging and accept the RSA prompt."
        )));
    }
    let mut device = DeviceInfo {
        serial: serial.clone(),
        name: serial.clone(),
        model: String::new(),
        android_version: String::new(),
        state: "device".into(),
        host: if serial.contains(':') {
            Some(serial.clone())
        } else {
            None
        },
        saved: true,
        last_connected_at: None,
    };
    devices::enrich(&adb, &mut device).await;
    state.registry.lock().await.remember(device.clone())?;
    *state.connected.lock().await = Some(device.clone());
    restart_logcat(app.clone(), state, serial.clone()).await;
    attach_inspect_proxy(&adb, &serial, state).await;
    emit_status(app, state).await;
    Ok(device)
}

#[tauri::command]
async fn pair_wireless(state: State<'_, Arc<AppState>>, host: String, code: String) -> Result<String> {
    let adb = state.adb.lock().await.clone();
    adb.pair(&host, &code).await
}

#[tauri::command]
async fn disconnect_device(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<()> {
    let serial = state
        .connected
        .lock()
        .await
        .take()
        .map(|d| d.serial);
    teardown_session(&state, serial.as_deref()).await;
    emit_status(&app, &state).await;
    Ok(())
}

#[tauri::command]
async fn forget_device(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    serial: String,
) -> Result<()> {
    let live = state
        .connected
        .lock()
        .await
        .as_ref()
        .map(|d| d.serial == serial)
        .unwrap_or(false);
    if live {
        let serial = state.connected.lock().await.take().map(|d| d.serial);
        teardown_session(&state, serial.as_deref()).await;
        emit_status(&app, &state).await;
    }
    state.registry.lock().await.forget(&serial)?;
    Ok(())
}

#[tauri::command]
async fn send_key(state: State<'_, Arc<AppState>>, command: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let (code, long) =
        command_to_keyevent(&command).ok_or_else(|| AppError::from("unknown command"))?;
    let adb = state.adb.lock().await.clone();
    if long {
        adb.shell(&serial, &format!("input keyevent --longpress {code}"))
            .await?;
    } else {
        adb.shell(&serial, &format!("input keyevent {code}")).await?;
    }
    Ok(())
}

#[tauri::command]
async fn keyboard_snapshot(state: State<'_, Arc<AppState>>) -> Result<KeyboardState> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    let input_method = adb
        .shell(&serial, "dumpsys input_method")
        .await
        .unwrap_or_default();
    let focus_line = adb
        .shell(&serial, "dumpsys window 2>/dev/null | grep mCurrentFocus")
        .await
        .unwrap_or_default();
    let mut dump = input_method;
    dump.push('\n');
    dump.push_str(&focus_line);
    let mut focused = keyboard_focused(&dump);
    let mut text = editor_text(&dump);
    let focus_hint = {
        let lower = focus_line.to_ascii_lowercase();
        lower.contains("inputmethod")
            || lower.contains("search")
            || lower.contains("edit")
            || lower.contains("keyboard")
            || lower.contains("ime")
    };
    if focused && text.is_empty() || !focused && focus_hint {
        let ui = adb
            .shell(
                &serial,
                "uiautomator dump /data/local/tmp/uidump.xml >/dev/null 2>&1 || cmd uiautomator dump /data/local/tmp/uidump.xml >/dev/null 2>&1; cat /data/local/tmp/uidump.xml 2>/dev/null",
            )
            .await
            .unwrap_or_default();
        dump.push('\n');
        dump.push_str(&ui);
        focused = keyboard_focused(&dump);
        if text.is_empty() {
            text = editor_text(&ui);
        }
    }
    Ok(KeyboardState { focused, text })
}

#[tauri::command]
async fn keyboard_set(
    state: State<'_, Arc<AppState>>,
    previous: String,
    text: String,
) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apply_field_text(&adb, &serial, &previous, &text).await
}

#[tauri::command]
async fn keyboard_submit(
    state: State<'_, Arc<AppState>>,
    previous: String,
    text: String,
) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apply_field_text(&adb, &serial, &previous, &text).await?;
    adb.shell(&serial, "input keyevent KEYCODE_ENTER").await?;
    Ok(())
}

async fn apply_field_text(
    adb: &AdbClient,
    serial: &str,
    previous: &str,
    next: &str,
) -> Result<()> {
    for action in field_update_actions(previous, next) {
        match action {
            InputAction::Text(payload) => {
                adb.shell(serial, &format!("input text {payload}")).await?;
            }
            InputAction::Keyevents(keys) => {
                for chunk in keys.chunks(24) {
                    adb.shell(serial, &format!("input keyevent {}", chunk.join(" ")))
                        .await?;
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
async fn keyboard_clear(state: State<'_, Arc<AppState>>) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    adb.shell(&serial, "input keyevent KEYCODE_CTRL_A").await?;
    adb.shell(&serial, "input keyevent KEYCODE_DEL").await?;
    Ok(())
}

#[tauri::command]
async fn now_playing(state: State<'_, Arc<AppState>>) -> Result<NowPlaying> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    let session = adb
        .shell(&serial, "dumpsys media_session")
        .await
        .unwrap_or_default();
    let (package, title, artist) = parse_now_playing(&session);
    Ok(NowPlaying {
        label: package.clone(),
        package_name: package,
        title,
        artist,
    })
}

#[tauri::command]
async fn start_stream(state: State<'_, Arc<AppState>>) -> Result<StreamStatus> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    let settings = state.settings.lock().await.clone();
    let config = scrcpy::StreamConfig {
        max_size: settings.max_size,
        video_bit_rate: settings.bit_rate,
        max_fps: settings.max_fps,
        audio: settings.audio_enabled,
    };
    state
        .scrcpy
        .clone()
        .start(adb, serial, state.resource_dir.clone(), config)
        .await
}

#[tauri::command]
async fn stop_stream(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.scrcpy.stop().await;
    Ok(())
}

#[tauri::command]
async fn stream_status(state: State<'_, Arc<AppState>>) -> Result<StreamStatus> {
    Ok(state.scrcpy.snapshot().await)
}

#[tauri::command]
async fn screenshot(state: State<'_, Arc<AppState>>) -> Result<String> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    let bytes = adb.exec_out(&serial, "screencap -p").await?;
    if bytes.len() < 32 {
        return Err(AppError::from("screenshot failed"));
    }
    let dir = state.settings.lock().await.capture_dir_path();
    paths::ensure_dir(&dir)?;
    let path = dir.join(format!(
        "screenshot-{}.png",
        chrono::Local::now().format("%Y-%m-%d-%H%M%S")
    ));
    std::fs::write(&path, bytes)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn start_recording(state: State<'_, Arc<AppState>>) -> Result<String> {
    let status = state.scrcpy.snapshot().await;
    if !status.streaming {
        return Err(AppError::from("not streaming"));
    }
    let dir = state.settings.lock().await.capture_dir_path();
    paths::ensure_dir(&dir)?;
    let path = dir.join(format!(
        "recording-{}.mp4",
        chrono::Local::now().format("%Y-%m-%d-%H%M%S")
    ));
    let sps = state.scrcpy.sps_snapshot().await;
    let pps = state.scrcpy.pps_snapshot().await;
    state
        .scrcpy
        .recorder
        .start(&path, status.width, status.height, sps, pps)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn stop_recording(state: State<'_, Arc<AppState>>) -> Result<String> {
    state.scrcpy.recorder.finish()
}

#[tauri::command]
async fn recording_status(state: State<'_, Arc<AppState>>) -> Result<RecordingStatus> {
    let (recording, elapsed_ms, bytes, path) = state.scrcpy.recorder.status();
    Ok(RecordingStatus {
        recording,
        elapsed_ms,
        bytes,
        path,
    })
}

#[tauri::command]
async fn reveal_path(path: String) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
    }
    Ok(())
}

#[tauri::command]
async fn start_logcat(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<()> {
    if let Ok(serial) = current_serial(&state).await {
        restart_logcat(app, &state, serial).await;
    }
    Ok(())
}

async fn restart_logcat(app: AppHandle, state: &AppState, serial: String) {
    if let Some(tx) = state.logcat_stop.lock().await.take() {
        let _ = tx.send(true);
    }
    let (tx, rx) = tokio::sync::watch::channel(false);
    *state.logcat_stop.lock().await = Some(tx);
    let adb = state.adb.lock().await.clone();
    let hub = state.logcat.clone();
    let crashes = state.crashes.clone();
    let serial1 = serial.clone();
    let reader_stop = rx.clone();
    tokio::spawn(async move {
        let _ = spawn_logcat(adb.clone(), serial1, hub.clone(), reader_stop).await;
    });
    let mut sub = state.logcat.subscribe();
    let app2 = app.clone();
    let log_state = state.log_buffer.clone();
    let crash_adb = state.adb.lock().await.clone();
    let network = state.network.clone();
    let mut fanout_stop = rx;
    tokio::spawn(async move {
        struct PendingCrash {
            kind: String,
            process: String,
            reason: String,
            stack: String,
            pid: i32,
            last: std::time::Instant,
        }
        let mut pending: Option<PendingCrash> = None;
        let flush_crash = |pending: &mut Option<PendingCrash>| {
            pending.take()
        };
        loop {
            tokio::select! {
                _ = fanout_stop.changed() => {
                    if *fanout_stop.borrow() {
                        break;
                    }
                }
                line = sub.recv() => {
                    let Ok(line) = line else { break; };
                    {
                        let mut buf = log_state.lock().await;
                        buf.push(line.clone());
                        if buf.len() > 20000 {
                            buf.drain(0..4000);
                        }
                    }
                    let _ = app2.emit("logcat", line.clone());

                    if let Some(http) = extract_http(&line) {
                        let mut headers = std::collections::HashMap::new();
                        headers.insert("X-Captured-From".into(), format!("logcat:{}", line.tag));
                        headers.insert("X-Log-Line".into(), line.message.clone());
                        let entry = NetworkEntry {
                            id: format!("log-{}", line.id),
                            started_at: chrono::Utc::now().timestamp_millis() as u64,
                            method: http.method,
                            url: http.url,
                            host: http.host,
                            path: http.path,
                            status: http.status,
                            duration_ms: http.duration_ms,
                            size: None,
                            encrypted: http.encrypted,
                            request_headers: headers,
                            response_headers: Default::default(),
                            request_body: None,
                            response_body: None,
                        };
                        let entry = network.upsert_from_log(entry);
                        let _ = app2.emit("network", entry);
                    }

                    let mut ready = None;
                    if let Some((kind, process, reason)) = is_crash_line(&line) {
                        ready = flush_crash(&mut pending);
                        pending = Some(PendingCrash {
                            kind: kind.to_string(),
                            process,
                            reason,
                            stack: format!("{} {}: {}", line.time, line.tag, line.message),
                            pid: line.pid,
                            last: std::time::Instant::now(),
                        });
                    } else if let Some(p) = pending.as_mut() {
                        if is_stack_followup(&line) {
                            p.stack.push('\n');
                            p.stack.push_str(&format!(
                                "{} {} {}: {}",
                                line.time, line.level, line.tag, line.message
                            ));
                            p.last = std::time::Instant::now();
                            if p.stack.lines().count() > 160 {
                                ready = flush_crash(&mut pending);
                            }
                        } else if p.last.elapsed() > Duration::from_millis(700) {
                            ready = flush_crash(&mut pending);
                        }
                    }
                    if let Some(p) = ready {
                        let stack =
                            crashes::enrich_from_device(&crash_adb, &serial, &p.stack).await;
                        let entry = crashes.push(&p.kind, p.process, p.reason, stack, p.pid);
                        let _ = app2.emit("crash", entry);
                    }
                }
            }
        }
        if let Some(p) = pending {
            let stack = crashes::enrich_from_device(&crash_adb, &serial, &p.stack).await;
            let entry = crashes.push(&p.kind, p.process, p.reason, stack, p.pid);
            let _ = app2.emit("crash", entry);
        }
    });
}

#[tauri::command]
async fn stop_logcat(state: State<'_, Arc<AppState>>) -> Result<()> {
    if let Some(tx) = state.logcat_stop.lock().await.take() {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
async fn clear_logcat(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.log_buffer.lock().await.clear();
    Ok(())
}

#[tauri::command]
async fn export_logcat(state: State<'_, Arc<AppState>>) -> Result<String> {
    let dir = state.settings.lock().await.capture_dir_path();
    paths::ensure_dir(&dir)?;
    let path = dir.join(format!(
        "logcat-{}.txt",
        chrono::Local::now().format("%Y-%m-%d-%H%M%S")
    ));
    let buf = state.log_buffer.lock().await;
    let body = buf
        .iter()
        .map(|l| {
            if l.raw.is_empty() {
                format!("{} {} {} {}: {}", l.time, l.level, l.pid, l.tag, l.message)
            } else {
                l.raw.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, body)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn list_crashes(state: State<'_, Arc<AppState>>) -> Result<Vec<crashes::CrashEntry>> {
    Ok(state.crashes.list())
}

#[tauri::command]
async fn save_crash(state: State<'_, Arc<AppState>>, id: String) -> Result<String> {
    state.crashes.save(&id)
}

#[tauri::command]
async fn start_proxy(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<u16> {
    let port = state.settings.lock().await.proxy_port;
    let log = state.network.clone();
    let app2 = app.clone();
    let bound = proxy::start_proxy(port, log, move |entry| {
        let _ = app2.emit("network", entry);
    })
    .await?;
    *state.proxy_port.lock().await = Some(bound);
    if let Ok(serial) = current_serial(&state).await {
        let adb = state.adb.lock().await.clone();
        attach_inspect_proxy(&adb, &serial, &state).await;
    }
    Ok(bound)
}

#[tauri::command]
async fn stop_proxy(state: State<'_, Arc<AppState>>) -> Result<()> {
    let mode = state.settings.lock().await.device_proxy_mode.clone();
    if let Ok(serial) = current_serial(&state).await {
        let adb = state.adb.lock().await.clone();
        if mode != "charles" && mode != "off" {
            let _ = adb.shell(&serial, "settings delete global http_proxy").await;
        }
        let _ = adb
            .run_serial(Some(&serial), &["reverse", "--remove-all"])
            .await;
    }
    *state.proxy_port.lock().await = None;
    Ok(())
}

#[tauri::command]
async fn clear_network(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.network.clear();
    Ok(())
}

#[tauri::command]
async fn export_har(state: State<'_, Arc<AppState>>) -> Result<String> {
    state.network.export_har()
}

#[tauri::command]
async fn list_network(state: State<'_, Arc<AppState>>) -> Result<Vec<proxy::NetworkEntry>> {
    Ok(state.network.list())
}

#[tauri::command]
async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
async fn save_settings(state: State<'_, Arc<AppState>>, settings: Settings) -> Result<()> {
    settings.save()?;
    *state.settings.lock().await = settings.clone();
    *state.adb.lock().await = rebuild_adb(&settings, &state.resource_dir);
    if let Ok(serial) = current_serial(&state).await {
        let adb = state.adb.lock().await.clone();
        attach_inspect_proxy(&adb, &serial, &state).await;
    }
    Ok(())
}

#[tauri::command]
async fn test_adb(state: State<'_, Arc<AppState>>) -> Result<String> {
    let adb = state.adb.lock().await.clone();
    let ver = adb.version().await?;
    let devices = adb.devices_raw().await?;
    Ok(format!("{ver}\n{devices}"))
}

#[tauri::command]
async fn inject_tap(
    state: State<'_, Arc<AppState>>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<()> {
    let serial = current_serial(&state).await?;
    let status = state.scrcpy.snapshot().await;
    if status.width == 0 || status.height == 0 || width == 0.0 || height == 0.0 {
        return Ok(());
    }
    let dx = (x / width) * status.width as f64;
    let dy = (y / height) * status.height as f64;
    let adb = state.adb.lock().await.clone();
    adb.shell(&serial, &format!("input tap {dx:.0} {dy:.0}"))
        .await?;
    Ok(())
}

#[tauri::command]
async fn refresh_all(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<ConnectionStatus> {
    if let Some(serial) = state
        .connected
        .lock()
        .await
        .as_ref()
        .map(|d| d.serial.clone())
    {
        let adb = state.adb.lock().await.clone();
        match adb.get_state(&serial).await {
            Ok(s) if s == "device" => {}
            _ => {
                *state.connected.lock().await = None;
                teardown_session(&state, Some(&serial)).await;
            }
        }
    }
    emit_status(&app, &state).await;
    build_status(&state).await
}

async fn auto_connect(app: &AppHandle, state: &AppState) {
    let adb = state.adb.lock().await.clone();
    let registry = state.registry.lock().await;
    if let Ok(list) = list_merged(&adb, &registry).await {
        if let Some(best) = list
            .into_iter()
            .filter(|d| d.state == "device")
            .max_by_key(|d| d.last_connected_at.unwrap_or(0))
        {
            drop(registry);
            let _ = connect_serial(app, state, best.serial).await;
        }
    }
}

#[tauri::command]
async fn list_apps(state: State<'_, Arc<AppState>>) -> Result<Vec<apps::AppInfo>> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apps::list_apps(&adb, &serial).await
}

#[tauri::command]
async fn launch_app(state: State<'_, Arc<AppState>>, package_name: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apps::launch_app(&adb, &serial, &package_name).await
}

#[tauri::command]
async fn force_stop_app(state: State<'_, Arc<AppState>>, package_name: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apps::force_stop_app(&adb, &serial, &package_name).await
}

#[tauri::command]
async fn install_apk(state: State<'_, Arc<AppState>>, file_path: String) -> Result<String> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apps::install_apk(&adb, &serial, &file_path).await
}

#[tauri::command]
async fn uninstall_app(state: State<'_, Arc<AppState>>, package_name: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    apps::uninstall_app(&adb, &serial, &package_name).await
}

#[tauri::command]
async fn list_files(state: State<'_, Arc<AppState>>, path: String) -> Result<Vec<files::RemoteFile>> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    files::list_files(&adb, &serial, &path).await
}

#[tauri::command]
async fn pull_file(state: State<'_, Arc<AppState>>, remote_path: String, local_path: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    let dest = {
        let path = std::path::PathBuf::from(&local_path);
        if path.is_absolute() {
            path
        } else {
            state.settings.lock().await.capture_dir_path().join(&local_path)
        }
    };
    if let Some(parent) = dest.parent() {
        paths::ensure_dir(parent)?;
    }
    files::pull_file(&adb, &serial, &remote_path, &dest.to_string_lossy()).await
}

#[tauri::command]
async fn push_file(state: State<'_, Arc<AppState>>, local_path: String, remote_path: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    files::push_file(&adb, &serial, &local_path, &remote_path).await
}

#[tauri::command]
async fn delete_file(state: State<'_, Arc<AppState>>, remote_path: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    files::delete_file(&adb, &serial, &remote_path).await
}

#[tauri::command]
async fn mkdir_remote(state: State<'_, Arc<AppState>>, remote_path: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    files::mkdir_remote(&adb, &serial, &remote_path).await
}

#[tauri::command]
async fn get_clipboard(state: State<'_, Arc<AppState>>) -> Result<String> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    clipboard::get_clipboard(&adb, &serial).await
}

#[tauri::command]
async fn set_clipboard(state: State<'_, Arc<AppState>>, text: String) -> Result<()> {
    let serial = current_serial(&state).await?;
    let adb = state.adb.lock().await.clone();
    clipboard::set_clipboard(&adb, &serial, &text).await
}

#[tauri::command]
async fn fix_port_5555(state: State<'_, Arc<AppState>>, serial: String) -> Result<String> {
    let adb = state.adb.lock().await.clone();
    adb.enable_tcpip(&serial, 5555).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let resource_dir = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let settings = Settings::load();
            let adb = rebuild_adb(&settings, &resource_dir);
            let state = Arc::new(AppState {
                settings: Mutex::new(settings),
                adb: Mutex::new(adb),
                registry: Mutex::new(DeviceRegistry::load()),
                connected: Mutex::new(None),
                scrcpy: Arc::new(ScrcpySession::new()),
                logcat: Arc::new(LogcatHub::new()),
                logcat_stop: Mutex::new(None),
                log_buffer: Arc::new(Mutex::new(Vec::new())),
                crashes: Arc::new(CrashLog::new()),
                network: Arc::new(NetworkLog::new()),
                resource_dir,
                proxy_port: Mutex::new(None),
            });
            app.manage(state.clone());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                auto_connect(&handle, &state).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            status,
            list_devices,
            connect_device,
            connect_host,
            pair_wireless,
            disconnect_device,
            forget_device,
            send_key,
            keyboard_snapshot,
            keyboard_set,
            keyboard_submit,
            keyboard_clear,
            now_playing,
            start_stream,
            stop_stream,
            stream_status,
            screenshot,
            start_recording,
            stop_recording,
            recording_status,
            reveal_path,
            start_logcat,
            stop_logcat,
            clear_logcat,
            export_logcat,
            list_crashes,
            save_crash,
            start_proxy,
            stop_proxy,
            clear_network,
            export_har,
            list_network,
            get_settings,
            save_settings,
            test_adb,
            inject_tap,
            refresh_all,
            list_apps,
            launch_app,
            force_stop_app,
            install_apk,
            uninstall_app,
            list_files,
            pull_file,
            push_file,
            delete_file,
            mkdir_remote,
            get_clipboard,
            set_clipboard,
            fix_port_5555
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    let state = state.inner().clone();
                    let _ = std::thread::spawn(move || {
                        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            rt.block_on(cleanup_on_exit(&state));
                        }
                    })
                    .join();
                }
            }
        });
}

async fn cleanup_on_exit(state: &AppState) {
    let serial = state.connected.lock().await.take().map(|d| d.serial);
    teardown_session(state, serial.as_deref()).await;
}

async fn teardown_session(state: &AppState, serial: Option<&str>) {
    if let Some(serial) = serial {
        let adb = state.adb.lock().await.clone();
        let mode = state.settings.lock().await.device_proxy_mode.clone();
        if mode != "charles" && mode != "off" {
            let _ = adb.shell(serial, "settings delete global http_proxy").await;
        }
        let _ = adb
            .run_serial(Some(serial), &["reverse", "--remove-all"])
            .await;
        if serial.contains(':') {
            let _ = adb.disconnect(serial).await;
        }
    }
    state.scrcpy.stop().await;
    if let Some(tx) = state.logcat_stop.lock().await.take() {
        let _ = tx.send(true);
    }
}

async fn attach_inspect_proxy(adb: &AdbClient, serial: &str, state: &AppState) {
    let settings = state.settings.lock().await.clone();
    match settings.device_proxy_mode.as_str() {
        "off" => {}
        "charles" => {
            let host = settings.charles_host.trim();
            if host.is_empty() {
                return;
            }
            let dest = format!("{host}:{}", settings.charles_port);
            let _ = adb
                .shell(serial, &format!("settings put global http_proxy {dest}"))
                .await;
            let _ = adb
                .shell(serial, &format!("settings put global global_http_proxy_host {host}"))
                .await;
            let _ = adb
                .shell(
                    serial,
                    &format!(
                        "settings put global global_http_proxy_port {}",
                        settings.charles_port
                    ),
                )
                .await;
            let _ = adb
                .shell(
                    serial,
                    "settings put global global_http_proxy_exclusion_list localhost,127.0.0.1",
                )
                .await;
        }
        _ => {
            let Some(port) = *state.proxy_port.lock().await else {
                return;
            };
            let bind = format!("tcp:{port}");
            let _ = adb.reverse(serial, &bind, &bind).await;
            let _ = adb
                .shell(
                    serial,
                    &format!("settings put global http_proxy 127.0.0.1:{port}"),
                )
                .await;
        }
    }
}
