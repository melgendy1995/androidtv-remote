import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  ConnectionStatus,
  CrashEntry,
  DeviceInfo,
  KeyboardState,
  NetworkEntry,
  NowPlaying,
  RecordingStatus,
  RemoteFile,
  Settings,
  StreamStatus,
} from "./types";

export const api = {
  status: () => invoke<ConnectionStatus>("status"),
  listDevices: () => invoke<DeviceInfo[]>("list_devices"),
  connect: (serial: string) => invoke<DeviceInfo>("connect_device", { serial }),
  connectHost: (host: string) => invoke<DeviceInfo>("connect_host", { host }),
  pairWireless: (host: string, code: string) =>
    invoke<string>("pair_wireless", { host, code }),
  disconnect: () => invoke<void>("disconnect_device"),
  forget: (serial: string) => invoke<void>("forget_device", { serial }),
  sendKey: (command: string) => invoke<void>("send_key", { command }),
  keyboardSnapshot: () => invoke<KeyboardState>("keyboard_snapshot"),
  keyboardSet: (previous: string, text: string) =>
    invoke<void>("keyboard_set", { previous, text }),
  keyboardSubmit: (previous: string, text: string) =>
    invoke<void>("keyboard_submit", { previous, text }),
  keyboardClear: () => invoke<void>("keyboard_clear"),
  nowPlaying: () => invoke<NowPlaying>("now_playing"),
  startStream: () => invoke<StreamStatus>("start_stream"),
  stopStream: () => invoke<void>("stop_stream"),
  streamStatus: () => invoke<StreamStatus>("stream_status"),
  screenshot: () => invoke<string>("screenshot"),
  startRecording: () => invoke<string>("start_recording"),
  stopRecording: () => invoke<string>("stop_recording"),
  recordingStatus: () => invoke<RecordingStatus>("recording_status"),
  reveal: (path: string) => invoke<void>("reveal_path", { path }),
  startLogcat: () => invoke<void>("start_logcat"),
  stopLogcat: () => invoke<void>("stop_logcat"),
  clearLogcat: () => invoke<void>("clear_logcat"),
  exportLogcat: () => invoke<string>("export_logcat"),
  listCrashes: () => invoke<CrashEntry[]>("list_crashes"),
  saveCrash: (id: string) => invoke<string>("save_crash", { id }),
  startProxy: () => invoke<number>("start_proxy"),
  stopProxy: () => invoke<void>("stop_proxy"),
  clearNetwork: () => invoke<void>("clear_network"),
  exportHar: () => invoke<string>("export_har"),
  listNetwork: () => invoke<NetworkEntry[]>("list_network"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    invoke<void>("save_settings", { settings }),
  testAdb: () => invoke<string>("test_adb"),
  injectTap: (x: number, y: number, width: number, height: number) =>
    invoke<void>("inject_tap", { x, y, width, height }),
  refresh: () => invoke<ConnectionStatus>("refresh_all"),

  // Phase 1: App Management
  listApps: () => invoke<AppInfo[]>("list_apps"),
  launchApp: (packageName: string) =>
    invoke<void>("launch_app", { packageName }),
  forceStopApp: (packageName: string) =>
    invoke<void>("force_stop_app", { packageName }),
  installApk: (filePath: string) =>
    invoke<string>("install_apk", { filePath }),
  uninstallApp: (packageName: string) =>
    invoke<void>("uninstall_app", { packageName }),

  // Phase 3: File Explorer
  listFiles: (path: string) => invoke<RemoteFile[]>("list_files", { path }),
  pullFile: (remotePath: string, localPath: string) =>
    invoke<void>("pull_file", { remotePath, localPath }),
  pushFile: (localPath: string, remotePath: string) =>
    invoke<void>("push_file", { localPath, remotePath }),
  deleteFile: (remotePath: string) =>
    invoke<void>("delete_file", { remotePath }),
  mkdirRemote: (remotePath: string) =>
    invoke<void>("mkdir_remote", { remotePath }),

  // Phase 2: Clipboard Sync
  getClipboard: () => invoke<string>("get_clipboard"),
  setClipboard: (text: string) => invoke<void>("set_clipboard", { text }),
  fixPort5555: (serial: string) => invoke<string>("fix_port_5555", { serial }),
};
