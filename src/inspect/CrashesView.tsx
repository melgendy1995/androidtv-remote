import { useState } from "react";
import type { CrashEntry } from "../types";

export function CrashesView({
  entries,
  onSave,
}: {
  entries: CrashEntry[];
  onSave: (id: string) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  if (entries.length === 0) {
    return <p className="hint" style={{ padding: 12 }}>No crashes or ANRs captured this session.</p>;
  }

  return (
    <div className="inspect-body" style={{ fontFamily: "inherit", padding: "4px 0" }}>
      {entries.map((c) => {
        const hidden = !!collapsed[c.id];
        return (
          <div key={c.id} style={{ borderBottom: "1px solid var(--border)", padding: "12px 14px" }}>
            <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
              <span className={c.kind === "anr" ? "badge warn" : "badge good"}>
                {c.kind.toUpperCase()}
              </span>
              <strong style={{ fontSize: 13 }}>{c.packageName || c.process}</strong>
              <span className="muted" style={{ fontSize: 11 }}>
                {new Date(c.at).toLocaleString()}
              </span>
              {c.pid ? (
                <span className="muted" style={{ fontSize: 11 }}>
                  PID {c.pid}
                </span>
              ) : null}
              <button
                className="surface-btn"
                style={{ width: "auto", padding: "2px 8px", marginLeft: "auto" }}
                onClick={() => onSave(c.id)}
              >
                Save
              </button>
              <button
                className="surface-btn"
                style={{ width: "auto", padding: "2px 8px" }}
                onClick={() => navigator.clipboard.writeText(formatCrash(c))}
              >
                Copy
              </button>
            </div>
            {c.exception ? (
              <div style={{ color: "#ff453a", fontWeight: 700, fontSize: 12, marginTop: 6 }}>
                {c.exception}
              </div>
            ) : null}
            <div className="hint" style={{ margin: "6px 0", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
              {c.reason}
            </div>
            <button
              className="accent"
              style={{ fontSize: 11 }}
              onClick={() => setCollapsed((m) => ({ ...m, [c.id]: !hidden }))}
            >
              {hidden ? "Show stack" : "Hide stack"}
            </button>
            {hidden ? null : (
              <pre
                style={{
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                  fontSize: 11,
                  marginTop: 8,
                  padding: 10,
                  background: "#060608",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  maxHeight: "50vh",
                  overflow: "auto",
                }}
              >
                {c.stack || "(no stack captured)"}
              </pre>
            )}
          </div>
        );
      })}
    </div>
  );
}

function formatCrash(c: CrashEntry) {
  return [
    `${c.kind.toUpperCase()}  ${c.packageName || c.process}`,
    c.exception,
    `PID ${c.pid || "—"}  ${new Date(c.at).toLocaleString()}`,
    c.reason,
    "",
    c.stack,
  ]
    .filter((line) => line != null && line !== "")
    .join("\n");
}
