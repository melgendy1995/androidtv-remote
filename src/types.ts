export type DeviceState = "offline" | "device" | "unauthorized" | "unknown";

export type DeviceInfo = {
  serial: string;
  name: string;
  model: string;
  androidVersion: string;
  state: DeviceState;
  host?: string;
  saved: boolean;
  lastConnectedAt?: number;
};

export type ConnectionStatus = {
  connected: boolean;
  serial?: string;
  device?: DeviceInfo;
  unauthorized: boolean;
  adbOk: boolean;
  adbVersion?: string;
  adbError?: string;
};

export type NowPlaying = {
  packageName: string;
  label: string;
  title: string;
  artist: string;
};

export type KeyboardState = {
  focused: boolean;
  text: string;
};

export type StreamStatus = {
  streaming: boolean;
  videoPort: number | null;
  width: number;
  height: number;
  lastFrameAt: number | null;
  error?: string;
};

export type CaptureToast = {
  kind: "screenshot" | "recording";
  path: string;
  name: string;
};

export type RecordingStatus = {
  recording: boolean;
  elapsedMs: number;
  bytes: number;
  path?: string;
};

export type LogLevel = "V" | "D" | "I" | "W" | "E" | "F";

export type LogLine = {
  id: number;
  time: string;
  pid: number;
  tid: number;
  level: LogLevel;
  tag: string;
  message: string;
  raw?: string;
};

export type CrashEntry = {
  id: string;
  at: number;
  kind: "crash" | "anr";
  process: string;
  reason: string;
  stack: string;
  pid?: number;
  exception?: string;
  packageName?: string;
};

export type NetworkEntry = {
  id: string;
  startedAt: number;
  method: string;
  url?: string;
  host: string;
  path: string;
  status?: number;
  durationMs?: number;
  size?: number;
  encrypted: boolean;
  requestHeaders: Record<string, string>;
  responseHeaders: Record<string, string>;
  requestBody?: string;
  responseBody?: string;
};

export type AppInfo = {
  packageName: string;
  label: string;
  isSystem: boolean;
};

export type RemoteFile = {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modified: string;
};

export type Settings = {
  adbPath: string;
  proxyPort: number;
  captureDir: string;
  maxSize: number;
  bitRate: number;
  maxFps: number;
  audioEnabled: boolean;
  deviceProxyMode?: "builtin" | "charles" | "off";
  charlesHost?: string;
  charlesPort?: number;
};

export type Sheet =
  | "devices"
  | "settings"
  | "keyboard"
  | "pair"
  | "apps"
  | "files"
  | null;
