import { useState, useMemo } from "react";
import type { NetworkEntry } from "../types";

function JsonSyntaxViewer({ data }: { data: any }) {
  const jsonStr = useMemo(() => {
    try {
      if (typeof data === "string") {
        const parsed = JSON.parse(data);
        return JSON.stringify(parsed, null, 2);
      }
      return JSON.stringify(data, null, 2);
    } catch {
      return String(data);
    }
  }, [data]);

  const htmlContent = useMemo(() => {
    if (!jsonStr) return "";
    const escaped = jsonStr
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    return escaped.replace(
      /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g,
      (match) => {
        let color = "#ffd60a"; // number
        let weight = 400;

        if (/^"/.test(match)) {
          if (/:$/.test(match)) {
            color = "#64d2ff"; // key
            weight = 600;
          } else {
            color = "#30d158"; // string
          }
        } else if (/true|false/.test(match)) {
          color = "#bf5af2"; // boolean
        } else if (/null/.test(match)) {
          color = "#ff453a"; // null
        }

        return `<span style="color: ${color}; font-weight: ${weight}">${match}</span>`;
      }
    );
  }, [jsonStr]);

  return (
    <pre
      style={{
        margin: 0,
        padding: 10,
        borderRadius: 8,
        background: "#060608",
        border: "1px solid var(--border)",
        fontSize: 11,
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        whiteSpace: "pre-wrap",
        wordBreak: "break-all",
        maxHeight: 280,
        overflowY: "auto",
      }}
      dangerouslySetInnerHTML={{ __html: htmlContent }}
    />
  );
}

function getStatusBadge(status?: number, encrypted?: boolean) {
  if (status == null) {
    if (encrypted) {
      return { fg: "#64d2ff", bg: "rgba(100,210,255,0.15)", label: "TLS Encrypted" };
    }
    return { fg: "#8e8e93", bg: "rgba(142,142,147,0.15)", label: "Pending…" };
  }

  if (status >= 200 && status < 300) {
    return { fg: "#30d158", bg: "rgba(48,209,88,0.18)", label: `${status} OK` };
  }
  if (status >= 300 && status < 400) {
    return { fg: "#ffd60a", bg: "rgba(255,214,10,0.2)", label: `${status} Redirect` };
  }
  if (status >= 400 && status < 500) {
    return { fg: "#ff453a", bg: "rgba(255,69,58,0.25)", label: `${status} Client Error` };
  }
  if (status >= 500) {
    return { fg: "#ff453a", bg: "rgba(255,69,58,0.3)", label: `${status} Server Error` };
  }
  return { fg: "#ff453a", bg: "rgba(255,69,58,0.2)", label: `${status}` };
}

function entryUrl(e: NetworkEntry) {
  if (e.url) return e.url;
  const scheme = e.encrypted ? "https" : "http";
  const host = (e.host || "").replace(/:443$/, "");
  const path = e.path?.startsWith("/") ? e.path : `/${e.path || ""}`;
  return `${scheme}://${host}${path}`;
}

function getMethodColor(method?: string) {
  switch ((method || "").toUpperCase()) {
    case "GET":
      return "#30d158";
    case "POST":
      return "#0a84ff";
    case "PUT":
      return "#ffd60a";
    case "DELETE":
      return "#ff453a";
    case "PATCH":
      return "#bf5af2";
    default:
      return "#8e8e93";
  }
}

export function NetworkView({
  entries,
  onClear,
  onExport,
}: {
  entries: NetworkEntry[];
  onClear: () => void;
  onExport: () => void;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<"all" | "errors" | "success">("all");
  const [query, setQuery] = useState("");

  const filteredEntries = useMemo(() => {
    const q = query.toLowerCase().trim();
    return (entries || []).filter((e) => {
      const isFailed = (e.status && e.status >= 400) || (!e.encrypted && e.status == null);
      if (filter === "errors" && !isFailed) return false;
      if (filter === "success" && (isFailed || (e.status && e.status >= 400))) return false;

      if (q) {
        const full = `${e.method || ""} ${entryUrl(e)} ${e.host || ""} ${e.path || ""} ${e.status || ""}`.toLowerCase();
        if (!full.includes(q)) return false;
      }
      return true;
    });
  }, [entries, filter, query]);

  const selectedRow = entries.find((e) => e.id === selectedId);

  return (
    <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
      {/* Network Request List */}
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        {/* Toolbar with status filters & search */}
        <div
          className="inspect-tools"
          style={{
            padding: "8px 12px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            gap: 8,
            background: "rgba(22, 22, 26, 0.7)",
            flexWrap: "wrap",
          }}
        >
          <input
            placeholder="Search URL / Path / Method…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            style={{ flex: 1, minWidth: 160 }}
          />

          <div style={{ display: "flex", gap: 4 }}>
            {(["all", "errors", "success"] as const).map((f) => (
              <button
                key={f}
                className="surface-btn"
                style={{
                  width: "auto",
                  padding: "4px 8px",
                  fontSize: 11,
                  background: filter === f ? "rgba(10,132,255,0.2)" : "rgba(255,255,255,0.06)",
                  color: filter === f ? "#0a84ff" : f === "errors" ? "#ff453a" : "inherit",
                  fontWeight: filter === f ? 600 : 400,
                }}
                onClick={() => setFilter(f)}
              >
                {f === "all" ? `All (${entries.length})` : f === "errors" ? "⚠️ Failed Only" : "✓ 2xx Success"}
              </button>
            ))}
          </div>

          <button className="surface-btn" style={{ width: "auto", padding: "4px 8px", fontSize: 11 }} onClick={onClear}>
            🗑 Clear
          </button>
          <button className="primary-btn" style={{ width: "auto", padding: "4px 10px", fontSize: 11 }} onClick={onExport}>
            📥 Export HAR
          </button>
        </div>

        {/* Requests Table Body */}
        <div className="inspect-body" style={{ flex: 1, overflowY: "auto", padding: "4px 0" }}>
          {filteredEntries.map((e, idx) => {
            const badge = getStatusBadge(e.status, e.encrypted);
            const methodColor = getMethodColor(e.method);
            const isFailed = (e.status && e.status >= 400) || (!e.encrypted && e.status == null);
            const isSelected = selectedId === e.id;

            return (
              <div
                key={e.id || idx}
                onClick={() => setSelectedId(isSelected ? null : e.id)}
                style={{
                  display: "grid",
                  gridTemplateColumns: "64px 1fr 110px 64px 64px",
                  gap: 8,
                  padding: "5px 12px",
                  alignItems: "center",
                  fontSize: 11,
                  cursor: "pointer",
                  background: isSelected
                    ? "rgba(10, 132, 255, 0.25)"
                    : isFailed
                    ? "rgba(255, 69, 58, 0.12)"
                    : idx % 2 === 0
                    ? "rgba(255, 255, 255, 0.015)"
                    : "transparent",
                  borderLeft: isSelected
                    ? "3px solid #0a84ff"
                    : isFailed
                    ? "3px solid #ff453a"
                    : "3px solid transparent",
                  borderBottom: "1px solid rgba(255, 255, 255, 0.03)",
                }}
              >
                <span style={{ color: methodColor, fontWeight: 700 }}>{e.method || "GET"}</span>
                <span
                  title={entryUrl(e)}
                  style={{
                    color: isFailed ? "#ff453a" : "var(--text)",
                    wordBreak: "break-all",
                    whiteSpace: "normal",
                    lineHeight: 1.35,
                    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
                    fontSize: 11,
                  }}
                >
                  {entryUrl(e)}
                </span>
                <span>
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 700,
                      color: badge.fg,
                      background: badge.bg,
                      padding: "2px 6px",
                      borderRadius: 4,
                    }}
                  >
                    {badge.label}
                  </span>
                </span>
                <span style={{ color: "var(--muted)", fontVariantNumeric: "tabular-nums" }}>
                  {e.durationMs != null ? `${e.durationMs}ms` : "—"}
                </span>
                <span style={{ color: "var(--muted)", fontVariantNumeric: "tabular-nums" }}>
                  {e.size != null ? `${e.size} B` : "—"}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Selected Request Detail Panel with Color-Coded JSON Syntax Viewer */}
      {selectedRow ? (
        <RequestDetail entry={selectedRow} onClose={() => setSelectedId(null)} />
      ) : null}
    </div>
  );
}

function RequestDetail({
  entry,
  onClose,
}: {
  entry: NetworkEntry;
  onClose: () => void;
}) {
  const [pane, setPane] = useState<"request" | "response">("request");
  const badge = getStatusBadge(entry.status, entry.encrypted);
  const url = entryUrl(entry);

  return (
    <div
      style={{
        width: "min(520px, 48%)",
        minWidth: 320,
        borderLeft: "1px solid var(--border)",
        background: "#0c0c0f",
        padding: 14,
        display: "flex",
        flexDirection: "column",
        gap: 10,
        overflowY: "auto",
        fontSize: 11,
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 8 }}>
        <div style={{ minWidth: 0 }}>
          <div style={{ color: getMethodColor(entry.method), fontWeight: 700, fontSize: 13 }}>
            {entry.method}
          </div>
          <div
            style={{
              color: "var(--text)",
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
              wordBreak: "break-all",
              marginTop: 4,
              lineHeight: 1.4,
            }}
          >
            {url}
          </div>
        </div>
        <button className="icon-btn" style={{ width: 22, height: 22, flexShrink: 0 }} onClick={onClose}>
          ×
        </button>
      </div>

      <div>
        <span
          style={{
            fontSize: 11,
            fontWeight: 700,
            color: badge.fg,
            background: badge.bg,
            padding: "3px 8px",
            borderRadius: 6,
          }}
        >
          {badge.label}
        </span>
        <span style={{ color: "var(--muted)", marginLeft: 8 }}>
          {entry.durationMs != null ? `${entry.durationMs} ms` : "—"}
          {entry.size != null ? ` · ${entry.size} B` : ""}
        </span>
      </div>

      <div style={{ display: "flex", gap: 4 }}>
        {(["request", "response"] as const).map((tab) => (
          <button
            key={tab}
            className="surface-btn"
            onClick={() => setPane(tab)}
            style={{
              width: "auto",
              padding: "4px 10px",
              fontSize: 11,
              fontWeight: pane === tab ? 700 : 400,
              background: pane === tab ? "rgba(10,132,255,0.2)" : "rgba(255,255,255,0.06)",
              color: pane === tab ? "#0a84ff" : "inherit",
            }}
          >
            {tab === "request" ? "Request" : "Response"}
          </button>
        ))}
      </div>

      {pane === "request" ? (
        <>
          <HeaderTable title="Request headers" headers={entry.requestHeaders} />
          <BodyBlock
            title="Request body"
            body={entry.requestBody}
            empty={
              entry.encrypted
                ? "HTTPS tunnel — request body is encrypted and cannot be shown."
                : "No request body."
            }
          />
        </>
      ) : (
        <>
          <HeaderTable title="Response headers" headers={entry.responseHeaders} />
          <BodyBlock
            title="Response body"
            body={entry.responseBody}
            empty={
              entry.encrypted
                ? "HTTPS tunnel — response body is encrypted and cannot be shown."
                : "No response body."
            }
          />
        </>
      )}
    </div>
  );
}

function HeaderTable({
  title,
  headers,
}: {
  title: string;
  headers?: Record<string, string>;
}) {
  const rows = Object.entries(headers || {});
  return (
    <div>
      <div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 6 }}>
        {title}
      </div>
      {rows.length === 0 ? (
        <div style={{ color: "var(--muted)" }}>None captured.</div>
      ) : (
        <div
          style={{
            border: "1px solid var(--border)",
            borderRadius: 8,
            overflow: "hidden",
          }}
        >
          {rows.map(([name, value]) => (
            <div
              key={name}
              style={{
                display: "grid",
                gridTemplateColumns: "140px 1fr",
                gap: 8,
                padding: "5px 8px",
                borderBottom: "1px solid rgba(255,255,255,0.04)",
                wordBreak: "break-all",
              }}
            >
              <span style={{ color: "#64d2ff", fontWeight: 600 }}>{name}</span>
              <span>{value}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function BodyBlock({
  title,
  body,
  empty,
}: {
  title: string;
  body?: string;
  empty: string;
}) {
  return (
    <div>
      <div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 6 }}>
        {title}
      </div>
      {body ? <JsonSyntaxViewer data={body} /> : <div style={{ color: "var(--muted)" }}>{empty}</div>}
    </div>
  );
}
