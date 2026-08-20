import { useState } from "react";
import type { CrashEntry } from "../types";

export function CrashesView({
  entries,
  onSave,
}: {
  entries: CrashEntry[];
  onSave: (id: string) => void;
}) {
  const [open, setOpen] = useState<string | null>(null);

  if (entries.length === 0) {
    return <p className="hint" style={{ padding: 12 }}>No crashes or ANRs captured this session.</p>;
  }

  return (
    <div className="inspect-body" style={{ fontFamily: "inherit" }}>
      {entries.map((c) => (
        <div key={c.id} style={{ borderBottom: "1px solid var(--border)", padding: 10 }}>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <span className={c.kind === "anr" ? "badge warn" : "badge good"}>
              {c.kind.toUpperCase()}
            </span>
            <strong style={{ fontSize: 12 }}>{c.process}</strong>
            <span className="muted" style={{ fontSize: 11 }}>
              {new Date(c.at).toLocaleTimeString()}
            </span>
            <button
              className="surface-btn"
              style={{ width: "auto", padding: "2px 8px", marginLeft: "auto" }}
              onClick={() => onSave(c.id)}
            >
              Save
            </button>
          </div>
          <div className="hint" style={{ margin: "4px 0" }}>
            {c.reason}
          </div>
          <button
            className="accent"
            style={{ fontSize: 11 }}
            onClick={() => setOpen(open === c.id ? null : c.id)}
          >
            {open === c.id ? "Hide stack" : "Show stack"}
          </button>
          {open === c.id ? (
            <pre style={{ whiteSpace: "pre-wrap", fontSize: 11 }}>{c.stack}</pre>
          ) : null}
        </div>
      ))}
    </div>
  );
}
