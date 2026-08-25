import { useState } from "react";
import type { CrashEntry } from "../types";
import { copyText } from "../utils/clipboard";

export function CrashesView({
  entries,
  onSave,
  onClear,
}: {
  entries: CrashEntry[];
  onSave: (id: string) => void;
  onClear: () => void;
}) {
  const [openStacks, setOpenStacks] = useState<Record<string, boolean>>({});
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const copyCrash = async (c: CrashEntry, kind: "full" | "stack") => {
    const text = kind === "stack" ? c.stack : formatCrash(c);
    if (text && (await copyText(text))) {
      setCopiedKey(`${c.id}:${kind}`);
      window.setTimeout(() => setCopiedKey(null), 1200);
    }
  };

  if (entries.length === 0) {
    return <p className="hint" style={{ padding: 12 }}>No crashes or ANRs captured this session.</p>;
  }

  return (
    <div className="inspect-body" style={{ fontFamily: "inherit", padding: "4px 0" }}>
      <div
        style={{
          display: "flex",
          justifyContent: "flex-end",
          padding: "6px 12px",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <button
          className="surface-btn"
          style={{ width: "auto", padding: "4px 10px", fontSize: 11 }}
          onClick={onClear}
        >
          🗑 Clear ({entries.length})
        </button>
      </div>
      {entries.map((c) => {
        const open = !!openStacks[c.id];
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
                style={{
                  width: "auto",
                  padding: "2px 8px",
                  color: copiedKey === `${c.id}:full` ? "#30d158" : "inherit",
                  fontWeight: copiedKey === `${c.id}:full` ? 700 : 400,
                }}
                onClick={() => copyCrash(c, "full")}
              >
                {copiedKey === `${c.id}:full` ? "✓ Copied" : "Copy"}
              </button>
            </div>
            {c.exception ? (
              <div style={{ color: "#ff453a", fontWeight: 700, fontSize: 12, marginTop: 6 }}>
                {c.exception}
              </div>
            ) : !open ? (
              <div
                className="hint"
                style={{
                  margin: "6px 0",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {c.reason}
              </div>
            ) : null}
            {open ? (
              <div className="hint" style={{ margin: "6px 0", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                {c.reason}
              </div>
            ) : null}
            <div style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 8 }}>
              <button
                className="surface-btn"
                style={{ width: "auto", padding: "4px 10px", fontSize: 11 }}
                onClick={() => setOpenStacks((m) => ({ ...m, [c.id]: !open }))}
              >
                {open ? "Hide stack trace" : "Show stack trace"}
              </button>
              {open && c.stack ? (
                <button
                  className="surface-btn"
                  style={{
                    width: "auto",
                    padding: "4px 10px",
                    fontSize: 11,
                    color: copiedKey === `${c.id}:stack` ? "#30d158" : "inherit",
                    fontWeight: copiedKey === `${c.id}:stack` ? 700 : 400,
                  }}
                  onClick={() => copyCrash(c, "stack")}
                >
                  {copiedKey === `${c.id}:stack` ? "✓ Copied" : "Copy stack"}
                </button>
              ) : null}
            </div>
            {open ? (
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
                  userSelect: "text",
                  cursor: "text",
                }}
              >
                {c.stack || "(no stack captured)"}
              </pre>
            ) : null}
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
