import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { CaptureBar } from "./capture/CaptureBar";
import { DevicePicker } from "./devices/DevicePicker";
import { PairWireless } from "./devices/PairWireless";
import { AppDrawer } from "./apps/AppDrawer";
import { FileManagerDrawer } from "./files/FileManagerDrawer";
import { InspectDrawer, type InspectTab } from "./inspect/InspectDrawer";
import { KeyboardSheet } from "./keyboard/KeyboardSheet";
import { CinemaLayout } from "./layout/CinemaLayout";
import { NowOnDevice } from "./now/NowOnDevice";
import { RemoteControls } from "./remote/RemoteControls";
import { useRemoteKeys } from "./remote/useRemoteKeys";
import { SettingsSheet } from "./settings/SettingsSheet";
import { StageView } from "./stage/StageView";
import type {
  CaptureToast,
  ConnectionStatus,
  CrashEntry,
  DeviceInfo,
  KeyboardState,
  LogLevel,
  LogLine,
  NetworkEntry,
  NowPlaying,
  RecordingStatus,
  Settings,
  Sheet,
  StreamStatus,
} from "./types";

const EMPTY_STREAM: StreamStatus = {
  streaming: false,
  videoPort: null,
  width: 0,
  height: 0,
  lastFrameAt: null,
};

const EMPTY_REC: RecordingStatus = {
  recording: false,
  elapsedMs: 0,
  bytes: 0,
};

export default function App() {
  const [status, setStatus] = useState<ConnectionStatus>({
    connected: false,
    unauthorized: false,
    adbOk: true,
  });
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [scanning, setScanning] = useState(false);
  const [connectingTo, setConnectingTo] = useState<string | null>(null);
  const [pickerError, setPickerError] = useState<string>();
  const [pairError, setPairError] = useState<string>();
  const [pairBusy, setPairBusy] = useState(false);
  const [sheet, setSheet] = useState<Sheet>(null);
  const [sidebarHidden, setSidebarHidden] = useState(false);
  const [inspectOpen, setInspectOpen] = useState(false);
  const [inspectTab, setInspectTab] = useState<InspectTab>("logs");
  const [now, setNow] = useState<NowPlaying | null>(null);
  const [keyboard, setKeyboard] = useState<KeyboardState>({
    focused: false,
    text: "",
  });
  const [keyboardAuto, setKeyboardAuto] = useState(false);
  const keyboardDismissedRef = useRef(false);
  const [stream, setStream] = useState<StreamStatus>(EMPTY_STREAM);
  const [recording, setRecording] = useState<RecordingStatus>(EMPTY_REC);
  const [toast, setToast] = useState<CaptureToast | null>(null);
  const [captureError, setCaptureError] = useState<string>();
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [logPaused, setLogPaused] = useState(false);
  const [logLevel, setLogLevel] = useState<LogLevel>("V");
  const [logQuery, setLogQuery] = useState("");
  const [logTag, setLogTag] = useState("");
  const [logPkg, setLogPkg] = useState("");
  const [network, setNetwork] = useState<NetworkEntry[]>([]);
  const [crashes, setCrashes] = useState<CrashEntry[]>([]);
  const [settings, setSettings] = useState<Settings>({
    adbPath: "",
    proxyPort: 8899,
    captureDir: "",
    maxSize: 1920,
    bitRate: 8000000,
    maxFps: 60,
    audioEnabled: false,
    deviceProxyMode: "builtin",
    charlesHost: "",
    charlesPort: 8888,
  });
  const [testResult, setTestResult] = useState<string>();
  const streamingSerial = useRef<string | null>(null);
  const logPausedRef = useRef(logPaused);
  logPausedRef.current = logPaused;

  const connected = status.connected;
  const sheetOpen = sheet != null;

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.status());
    } catch (e) {
      setStatus((s) => ({ ...s, adbOk: false, adbError: String(e) }));
    }
  }, []);

  const scan = useCallback(async (clear = false) => {
    setScanning(true);
    if (clear) setDevices([]);
    try {
      setDevices(await api.listDevices());
    } catch (e) {
      setPickerError(String(e));
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    scan();
    api.getSettings().then(setSettings).catch(() => undefined);
    api.startLogcat().catch(() => undefined);
    api.startProxy().catch(() => undefined);
  }, [refreshStatus, scan]);

  useEffect(() => {
    let dead = false;
    const unsubs: Array<() => void> = [];
    const add = async <T,>(name: string, fn: (p: T) => void) => {
      const u = await listen<T>(name, (e) => fn(e.payload));
      if (dead) {
        u();
        return;
      }
      unsubs.push(u);
    };
    add<ConnectionStatus>("status", setStatus);
    add<NowPlaying>("now-playing", setNow);
    add<KeyboardState>("keyboard", setKeyboard);
    add<StreamStatus>("stream", setStream);
    add<RecordingStatus>("recording", setRecording);
    add<LogLine>("logcat", (line) => {
      if (logPausedRef.current) return;
      setLogs((prev) => {
        const next = prev.length > 20000 ? prev.slice(-16000) : prev.slice();
        next.push(line);
        return next;
      });
    });
    add<NetworkEntry>("network", (entry) => {
      setNetwork((prev) => {
        const idx = prev.findIndex((e) => e.id === entry.id);
        if (idx >= 0) {
          const copy = prev.slice();
          copy[idx] = entry;
          return copy;
        }
        return [...prev, entry].slice(-2000);
      });
    });
    add<CrashEntry>("crash", (c) => setCrashes((prev) => [c, ...prev].slice(0, 200)));
    return () => {
      dead = true;
      unsubs.forEach((u) => u());
    };
  }, []);

  useEffect(() => {
    const onToggle = () => setInspectOpen((v) => !v);
    window.addEventListener("toggle-inspect", onToggle);
    return () => window.removeEventListener("toggle-inspect", onToggle);
  }, []);

  useEffect(() => {
    if (!connected) {
      keyboardDismissedRef.current = false;
      setKeyboard({ focused: false, text: "" });
      setKeyboardAuto(false);
      setSheet((s) => (s === "keyboard" ? null : s));
      return;
    }
    let cancelled = false;
    let inFlight = false;
    let focusedTicks = 0;
    let unfocusedTicks = 0;
    const tick = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const snap = await api.keyboardSnapshot();
        if (cancelled) return;
        if (snap.focused) {
          unfocusedTicks = 0;
          focusedTicks += 1;
          if (focusedTicks >= 2) setKeyboard(snap);
        } else {
          focusedTicks = 0;
          unfocusedTicks += 1;
          if (unfocusedTicks >= 2) setKeyboard(snap);
        }
      } catch {
        /* ignore transient adb errors */
      } finally {
        inFlight = false;
      }
    };
    tick();
    const timer = window.setInterval(tick, 600);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [connected]);

  const closeKeyboard = useCallback(() => {
    // Stay closed until the TV field actually blurs; otherwise Close immediately reopens.
    keyboardDismissedRef.current = true;
    setKeyboardAuto(false);
    setSheet(null);
  }, []);

  useEffect(() => {
    if (!keyboard.focused) {
      keyboardDismissedRef.current = false;
      if (keyboardAuto && sheet === "keyboard") {
        setSheet(null);
        setKeyboardAuto(false);
      }
      return;
    }
    if (keyboardDismissedRef.current) return;
    if (sheet && sheet !== "keyboard") return;
    setSheet("keyboard");
    setKeyboardAuto(true);
  }, [keyboard.focused, keyboardAuto, sheet]);

  useEffect(() => {
    if (!connected || !status.serial) {
      const was = streamingSerial.current;
      streamingSerial.current = null;
      setStream(EMPTY_STREAM);
      if (was) api.stopStream().catch(() => undefined);
      return;
    }
    const serial = status.serial;
    if (streamingSerial.current === serial) return;
    streamingSerial.current = serial;
    api.startStream().then((next) => {
      if (streamingSerial.current === serial) setStream(next);
    }).catch((error) => {
      if (streamingSerial.current === serial) {
        streamingSerial.current = null;
        setCaptureError(String(error));
      }
    });
  }, [connected, status.serial]);

  useEffect(() => {
    if (!connected || !status.serial) return;
    const serial = status.serial;
    const poll = () => {
      api.nowPlaying().then(setNow).catch(() => undefined);
      api.streamStatus().then((next) => {
        if (streamingSerial.current === serial) setStream(next);
      }).catch(() => undefined);
    };
    poll();
    const timer = window.setInterval(poll, 2000);
    return () => window.clearInterval(timer);
  }, [connected, status.serial]);

  useEffect(() => {
    if (!inspectOpen) return;
    api.listNetwork().then(setNetwork).catch(() => undefined);
  }, [inspectOpen]);

  useEffect(() => {
    if (!recording.recording) return;
    const timer = window.setInterval(() => {
      api.recordingStatus().then(setRecording).catch(() => undefined);
    }, 250);
    return () => window.clearInterval(timer);
  }, [recording.recording]);

  const sendCommand = useCallback(
    (command: string) => {
      if (!connected) return;
      api.sendKey(command).catch(() => undefined);
    },
    [connected]
  );

  const snap = useCallback(async () => {
    try {
      const path = await api.screenshot();
      setToast({ kind: "screenshot", path, name: path.split(/[\\/]/).pop() || path });
      setCaptureError(undefined);
      api.reveal(path).catch(() => undefined);
    } catch (e) {
      setCaptureError(String(e));
    }
  }, []);

  const toggleRecord = useCallback(async () => {
    try {
      if (recording.recording) {
        const path = await api.stopRecording();
        setToast({ kind: "recording", path, name: path.split(/[\\/]/).pop() || path });
        api.reveal(path).catch(() => undefined);
      } else {
        await api.startRecording();
      }
      setRecording(await api.recordingStatus());
      setCaptureError(undefined);
    } catch (e) {
      setCaptureError(String(e));
    }
  }, [recording.recording]);

  useRemoteKeys({
    enabled: connected,
    sheetOpen,
    streaming: stream.streaming,
    onCommand: sendCommand,
    onScreenshot: snap,
    onToggleRecord: toggleRecord,
  });

  const connectDevice = async (serial: string) => {
    setConnectingTo(serial);
    setPickerError(undefined);
    try {
      await api.connect(serial);
      await refreshStatus();
      setSheet(null);
    } catch (e) {
      setPickerError(String(e));
    } finally {
      setConnectingTo(null);
    }
  };

  const disconnectDevice = useCallback(async () => {
    try {
      await api.disconnect();
      await refreshStatus();
      await scan();
    } catch (e) {
      setPickerError(String(e));
    }
  }, [refreshStatus, scan]);

  const addHost = async (host: string) => {
    setConnectingTo(host);
    setPickerError(undefined);
    try {
      await api.connectHost(host);
      await scan();
      await refreshStatus();
      setSheet(null);
    } catch (e) {
      setPickerError(String(e));
    } finally {
      setConnectingTo(null);
    }
  };

  const forget = async (serial: string) => {
    if (!window.confirm(`Forget ${serial}?`)) return;
    await api.forget(serial);
    await scan();
    await refreshStatus();
  };

  const refreshAll = async () => {
    try {
      await api.refresh();
      await refreshStatus();
      if (connected) {
        await api.stopStream().catch(() => undefined);
        setStream(await api.startStream());
      }
    } catch (e) {
      setCaptureError(String(e));
    }
  };

  const banner = useMemo(() => {
    if (!status.adbOk) return status.adbError || "ADB is not available.";
    if (status.unauthorized) return "Accept the RSA prompt on the TV.";
    return null;
  }, [status]);

  return (
    <>
      <CinemaLayout
        sidebarHidden={sidebarHidden}
        inspectOpen={inspectOpen}
        onShowSidebar={() => setSidebarHidden(false)}
        stage={
          <StageView
            connected={connected}
            deviceName={status.device?.name}
            stream={stream}
            recording={recording}
            onTap={(x, y, w, h) => {
              api.injectTap(x, y, w, h)
                .then(() => new Promise<void>((resolve) => window.setTimeout(resolve, 280)))
                .then(() => api.keyboardSnapshot())
                .then(setKeyboard)
                .catch(() => undefined);
            }}
          />
        }
        sidebar={
          <>
            {/* Row 1: Device Connection Status Header */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 8,
                padding: "4px 0",
              }}
            >
              <div
                className="status-pill"
                onClick={() => setSheet("devices")}
                style={{
                  cursor: "pointer",
                  background: "rgba(255, 255, 255, 0.05)",
                  padding: "6px 12px",
                  borderRadius: 999,
                  border: "1px solid var(--border)",
                  flex: 1,
                  minWidth: 0,
                }}
                title="Click to view & switch devices"
              >
                <span className={`status-dot${connected ? " live" : ""}`} />
                <span
                  className="status-name"
                  style={{
                    fontWeight: 600,
                    fontSize: 13,
                    color: connected ? "var(--text)" : "var(--muted)",
                  }}
                >
                  {status.device?.name || (connected ? "Connected TV" : "No device connected")}
                </span>
              </div>

              <div style={{ display: "flex", gap: 4 }}>
                <button
                  className="icon-btn glass"
                  title="Devices Picker (⋮)"
                  onClick={() => setSheet("devices")}
                  style={{ width: 32, height: 32, fontSize: 14 }}
                >
                  ⋮
                </button>
                <button
                  className="icon-btn glass"
                  title="Hide Remote (»)"
                  onClick={() => setSidebarHidden(true)}
                  style={{ width: 32, height: 32, fontSize: 14 }}
                >
                  »
                </button>
              </div>
            </div>

            {/* Row 2: Quick Feature Action Toolstrip */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 4,
                padding: "4px 0",
                borderBottom: "1px solid var(--border)",
                paddingBottom: 10,
              }}
            >
              <button
                className="icon-btn glass"
                title="Apps & Sideloading"
                disabled={!connected}
                onClick={() => setSheet("apps")}
                style={{ flex: 1, height: 34 }}
              >
                🚀
              </button>
              <button
                className="icon-btn glass"
                title="ADB File Explorer"
                disabled={!connected}
                onClick={() => setSheet("files")}
                style={{ flex: 1, height: 34 }}
              >
                📁
              </button>
              <button
                className="icon-btn glass"
                title="Keyboard & Clipboard"
                disabled={!connected}
                onClick={() => {
                  setKeyboardAuto(false);
                  setSheet("keyboard");
                }}
                style={{ flex: 1, height: 34 }}
              >
                ⌨
              </button>
              <button
                className="icon-btn glass"
                title="Inspect Logcat (`)"
                onClick={() => setInspectOpen((v) => !v)}
                style={{ flex: 1, height: 34 }}
              >
                ☰
              </button>
              <button
                className="icon-btn glass"
                title="Refresh connection & screen"
                onClick={refreshAll}
                style={{ flex: 1, height: 34 }}
              >
                ↻
              </button>
              <button
                className="icon-btn glass"
                title="Settings"
                onClick={() => setSheet("settings")}
                style={{ flex: 1, height: 34 }}
              >
                ⚙
              </button>
            </div>
            {banner ? (
              <div className="banner danger">
                {banner}{" "}
                <button className="accent" onClick={() => setSheet("settings")}>
                  Settings
                </button>
              </div>
            ) : null}
            <NowOnDevice now={now} />
            <RemoteControls connected={connected} onCommand={sendCommand} />
            <div style={{ height: 1, background: "var(--border)" }} />
            <CaptureBar
              streaming={stream.streaming}
              recording={recording.recording}
              toast={toast}
              error={captureError || stream.error}
              onSnap={snap}
              onToggleRecord={toggleRecord}
              onReveal={(path) => api.reveal(path)}
            />
            <div className="grow" />
          </>
        }
        inspect={
          <InspectDrawer
            tab={inspectTab}
            onTab={setInspectTab}
            onClose={() => setInspectOpen(false)}
            logs={{
              lines: logs,
              paused: logPaused,
              level: logLevel,
              query: logQuery,
              tag: logTag,
              pkg: logPkg,
              onLevel: setLogLevel,
              onQuery: setLogQuery,
              onTag: setLogTag,
              onPkg: setLogPkg,
              onPause: () => setLogPaused((v) => !v),
              onClear: () => {
                setLogs([]);
                api.clearLogcat().catch(() => undefined);
              },
              onExport: () => {
                api.exportLogcat().then((p) => api.reveal(p)).catch((e) => setCaptureError(String(e)));
              },
            }}
            network={{
              entries: network,
              onClear: () => {
                setNetwork([]);
                api.clearNetwork().catch(() => undefined);
              },
              onExport: () => {
                api.exportHar().then((p) => api.reveal(p)).catch((e) => setCaptureError(String(e)));
              },
            }}
            crashes={{
              entries: crashes,
              onSave: (id) => {
                api.saveCrash(id).then((p) => api.reveal(p)).catch((e) => setCaptureError(String(e)));
              },
            }}
          />
        }
      />

      {sheet === "devices" ? (
        <DevicePicker
          devices={devices}
          liveSerial={status.serial}
          scanning={scanning}
          connectingTo={connectingTo}
          error={pickerError}
          onClose={() => setSheet(null)}
          onRescan={() => scan(true)}
          onConnect={connectDevice}
          onDisconnect={disconnectDevice}
          onFixPort={async (serial) => {
            try {
              await api.fixPort5555(serial);
              setPickerError(undefined);
              await scan();
            } catch (e) {
              setPickerError(String(e));
            }
          }}
          onForget={forget}
          onAddHost={addHost}
          onPair={() => setSheet("pair")}
        />
      ) : null}
      {sheet === "pair" ? (
        <PairWireless
          busy={pairBusy}
          error={pairError}
          onClose={() => setSheet("devices")}
          onPair={async (host, code) => {
            setPairBusy(true);
            setPairError(undefined);
            try {
              await api.pairWireless(host, code);
              const base = host.split(":")[0];
              await api.connectHost(`${base}:5555`);
              await scan();
              await refreshStatus();
              setSheet(null);
            } catch (e) {
              setPairError(String(e));
            } finally {
              setPairBusy(false);
            }
          }}
        />
      ) : null}
      {sheet === "keyboard" ? (
        <KeyboardSheet
          snapshot={keyboard}
          focused={keyboard.focused}
          onClose={closeKeyboard}
          onChange={(previous, text) =>
            api.keyboardSet(previous, text).catch(() => undefined)
          }
          onSubmit={(previous, text) => {
            api.keyboardSubmit(previous, text).then(closeKeyboard).catch(() => undefined);
          }}
        />
      ) : null}
      {sheet === "settings" ? (
        <SettingsSheet
          initial={settings}
          testResult={testResult}
          onClose={() => setSheet(null)}
          onSave={async (next) => {
            await api.saveSettings(next);
            setSettings(next);
            setSheet(null);
          }}
          onTest={async () => {
            try {
              setTestResult(await api.testAdb());
            } catch (e) {
              setTestResult(String(e));
            }
          }}
        />
      ) : null}
      {sheet === "apps" ? (
        <AppDrawer open={true} onClose={() => setSheet(null)} />
      ) : null}
      {sheet === "files" ? (
        <FileManagerDrawer open={true} onClose={() => setSheet(null)} />
      ) : null}
    </>
  );
}
