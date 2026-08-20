import { useState } from "react";
import type { Settings } from "../types";

export function SettingsSheet({
  initial,
  testResult,
  onClose,
  onSave,
  onTest,
}: {
  initial: Settings;
  testResult?: string;
  onClose: () => void;
  onSave: (settings: Settings) => void;
  onTest: () => void;
}) {
  const [settings, setSettings] = useState<Settings>({
    adbPath: initial.adbPath || "",
    proxyPort: initial.proxyPort || 8899,
    captureDir: initial.captureDir || "",
    maxSize: initial.maxSize ?? 1920,
    bitRate: initial.bitRate ?? 8000000,
    maxFps: initial.maxFps ?? 60,
    audioEnabled: initial.audioEnabled ?? false,
    deviceProxyMode: initial.deviceProxyMode || "builtin",
    charlesHost: initial.charlesHost || "",
    charlesPort: initial.charlesPort || 8888,
  });

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet glass" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 520 }}>
        <div className="sheet-toolbar">
          <button className="surface-btn" style={{ width: "auto" }} onClick={onClose}>
            Close
          </button>
          <h2>Settings & Stream Quality</h2>
          <span />
        </div>

        <label className="hint">ADB Binary Path (blank = bundled / PATH)</label>
        <input
          className="field mono"
          value={settings.adbPath}
          placeholder="/opt/homebrew/share/android-commandlinetools/platform-tools/adb"
          onChange={(e) => setSettings({ ...settings, adbPath: e.target.value })}
        />

        <div className="row" style={{ gap: 12, marginTop: 8 }}>
          <div style={{ flex: 1 }}>
            <label className="hint">Max Stream Resolution</label>
            <select
              className="field"
              value={settings.maxSize}
              onChange={(e) => setSettings({ ...settings, maxSize: Number(e.target.value) })}
            >
              <option value={1280}>720p (Fast / Low Latency)</option>
              <option value={1920}>1080p (Balanced - Recommended)</option>
              <option value={0}>Native TV Resolution</option>
            </select>
          </div>

          <div style={{ flex: 1 }}>
            <label className="hint">Video Bitrate</label>
            <select
              className="field"
              value={settings.bitRate}
              onChange={(e) => setSettings({ ...settings, bitRate: Number(e.target.value) })}
            >
              <option value={2000000}>2 Mbps (Low)</option>
              <option value={8000000}>8 Mbps (Standard)</option>
              <option value={16000000}>16 Mbps (High Quality)</option>
            </select>
          </div>
        </div>

        <div className="row" style={{ gap: 12, marginTop: 8 }}>
          <div style={{ flex: 1 }}>
            <label className="hint">Max Target FPS</label>
            <select
              className="field"
              value={settings.maxFps}
              onChange={(e) => setSettings({ ...settings, maxFps: Number(e.target.value) })}
            >
              <option value={30}>30 FPS</option>
              <option value={60}>60 FPS</option>
            </select>
          </div>

          <div style={{ flex: 1 }}>
            <label className="hint">TV HTTP proxy</label>
            <select
              className="field"
              value={settings.deviceProxyMode || "builtin"}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  deviceProxyMode: e.target.value as Settings["deviceProxyMode"],
                })
              }
            >
              <option value="builtin">Built-in Network tab</option>
              <option value="charles">Charles (keep remote + Charles)</option>
              <option value="off">Don’t change TV proxy</option>
            </select>
          </div>
        </div>

        {settings.deviceProxyMode === "charles" ? (
          <div className="row" style={{ gap: 12, marginTop: 8 }}>
            <div style={{ flex: 1 }}>
              <label className="hint">Charles host (this Mac’s LAN IP)</label>
              <input
                className="field mono"
                value={settings.charlesHost || ""}
                placeholder="192.168.1.161"
                onChange={(e) => setSettings({ ...settings, charlesHost: e.target.value })}
              />
            </div>
            <div style={{ width: 120 }}>
              <label className="hint">Charles port</label>
              <input
                className="field mono"
                value={settings.charlesPort || 8888}
                onChange={(e) =>
                  setSettings({ ...settings, charlesPort: Number(e.target.value) || 8888 })
                }
              />
            </div>
          </div>
        ) : settings.deviceProxyMode !== "off" ? (
          <div className="row" style={{ gap: 12, marginTop: 8 }}>
            <div style={{ flex: 1 }}>
              <label className="hint">Inspect Proxy Port</label>
              <input
                className="field mono"
                value={settings.proxyPort}
                onChange={(e) =>
                  setSettings({ ...settings, proxyPort: Number(e.target.value) || 8899 })
                }
              />
            </div>
          </div>
        ) : null}

        <label className="hint" style={{ marginTop: 8 }}>Capture Directory</label>
        <input
          className="field mono"
          value={settings.captureDir}
          onChange={(e) => setSettings({ ...settings, captureDir: e.target.value })}
        />

        <div className="row" style={{ marginTop: 14, alignItems: "center" }}>
          <button className="surface-btn" onClick={onTest}>
            Test ADB
          </button>
          <button className="primary-btn" onClick={() => onSave(settings)}>
            Save Settings
          </button>
        </div>

        {testResult ? <p className="hint">{testResult}</p> : null}
      </div>
    </div>
  );
}
