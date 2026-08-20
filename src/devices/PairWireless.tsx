import { useState } from "react";

export function PairWireless({
  busy,
  error,
  onClose,
  onPair,
}: {
  busy: boolean;
  error?: string;
  onClose: () => void;
  onPair: (host: string, code: string) => void;
}) {
  const [host, setHost] = useState("");
  const [code, setCode] = useState("");

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet glass" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-toolbar">
          <button className="surface-btn" style={{ width: "auto" }} onClick={onClose}>
            Close
          </button>
          <h2>Pair wireless</h2>
          <span />
        </div>
        <p className="hint">
          On the TV open Developer options → Wireless debugging → Pair device
          with pairing code. Enter the IP:port shown there and the 6-digit code.
        </p>
        <input
          className="field"
          placeholder="192.168.93.84:37123"
          value={host}
          onChange={(e) => setHost(e.target.value)}
        />
        <input
          className="field"
          style={{ marginTop: 8, textAlign: "center", fontSize: 20, letterSpacing: 4 }}
          placeholder="123456"
          value={code}
          onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
        />
        <button
          className="primary-btn"
          style={{ marginTop: 14 }}
          disabled={busy || !host.trim() || code.length < 6}
          onClick={() => onPair(host.trim(), code)}
        >
          {busy ? "Pairing…" : "Pair"}
        </button>
        {error ? <div className="error-line" style={{ marginTop: 10 }}>{error}</div> : null}
      </div>
    </div>
  );
}
