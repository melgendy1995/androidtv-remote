import { useState } from "react";
import type { DeviceInfo } from "../types";

export function DevicePicker({
  devices,
  liveSerial,
  scanning,
  connectingTo,
  error,
  onClose,
  onRescan,
  onConnect,
  onDisconnect,
  onFixPort,
  onForget,
  onAddHost,
  onPair,
}: {
  devices: DeviceInfo[];
  liveSerial?: string;
  scanning: boolean;
  connectingTo: string | null;
  error?: string;
  onClose: () => void;
  onRescan: () => void;
  onConnect: (serial: string) => void;
  onDisconnect: () => void;
  onFixPort: (serial: string) => void;
  onForget: (serial: string) => void;
  onAddHost: (host: string) => void;
  onPair: () => void;
}) {
  const [host, setHost] = useState("");
  const [fixing, setFixing] = useState<string | null>(null);

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet glass" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-toolbar">
          <button className="surface-btn" style={{ width: "auto" }} onClick={onClose}>
            Close
          </button>
          <h2>Android TVs</h2>
          <button
            className="surface-btn"
            style={{ width: "auto" }}
            disabled={scanning}
            onClick={onRescan}
          >
            Rescan
          </button>
        </div>

        {scanning && devices.length === 0 ? (
          <p className="hint" style={{ textAlign: "center", paddingTop: 30 }}>
            Scanning…
          </p>
        ) : null}
        {!scanning && devices.length === 0 ? (
          <p className="hint">
            No devices found.
            <br />
            Enable USB or wireless debugging, or add the TV by IP.
          </p>
        ) : null}

        {devices.map((d) => (
          <div key={d.serial} className="device-row glass glass-card-sm">
            <div className="device-meta">
              <div className="device-name">{d.name}</div>
              <div className="device-sub">
                {d.serial}
                {d.model ? ` · ${d.model}` : ""}
                {d.androidVersion ? ` · Android ${d.androidVersion}` : ""}
              </div>
            </div>
            {d.state === "unauthorized" ? (
              <span className="badge warn">Unauthorized</span>
            ) : d.serial === liveSerial ? (
              <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                <span className="good" style={{ fontSize: 12, fontWeight: 600 }}>
                  ✓ Connected
                </span>
                <button
                  className="surface-btn"
                  disabled={fixing === d.serial}
                  onClick={async () => {
                    setFixing(d.serial);
                    await onFixPort(d.serial);
                    setFixing(null);
                  }}
                  title="Lock TV ADB to permanent port 5555 across TV restarts"
                  style={{ fontSize: "0.75rem", padding: "4px 8px", width: "auto", background: "rgba(10,132,255,0.2)", color: "#0a84ff" }}
                >
                  {fixing === d.serial ? "Locking…" : "🔒 Lock Port 5555"}
                </button>
                <button
                  className="surface-btn"
                  onClick={onDisconnect}
                  style={{ fontSize: "0.75rem", padding: "4px 8px", width: "auto", background: "rgba(255,69,58,0.2)", color: "#ff453a" }}
                >
                  Disconnect
                </button>
              </div>
            ) : (
              <button
                className="connect-chip accent"
                disabled={connectingTo != null}
                onClick={() => onConnect(d.serial)}
              >
                {connectingTo === d.serial ? "…" : "Connect"}
              </button>
            )}
            {d.saved ? (
              <button
                className="icon-btn"
                title="Forget"
                onClick={() => onForget(d.serial)}
              >
                ⋯
              </button>
            ) : null}
          </div>
        ))}

        <p className="hint">
          Click <b>🔒 Lock Port 5555</b> to prevent port changes when your TV restarts.
        </p>

        <div className="glass glass-card-sm" style={{ padding: 12 }}>
          <div className="muted" style={{ fontSize: 11, fontWeight: 600 }}>
            Not seeing your Android TV?
          </div>
          <p className="hint">
            Wireless ADB uses <code>IP:5555</code>. If the TV needs pairing
            (Android 11+), use Pair wireless.
          </p>
          <div className="row">
            <input
              className="field"
              placeholder="192.168.93.84 or 192.168.93.84:5555"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && host.trim()) onAddHost(host.trim());
              }}
            />
            <button
              className="connect-chip accent"
              disabled={!host.trim() || connectingTo != null}
              onClick={() => onAddHost(host.trim())}
            >
              Add
            </button>
          </div>
          <button
            className="surface-btn"
            style={{ marginTop: 8 }}
            onClick={onPair}
          >
            Pair wireless…
          </button>
        </div>
        {error ? <div className="error-line" style={{ marginTop: 10 }}>{error}</div> : null}
      </div>
    </div>
  );
}
