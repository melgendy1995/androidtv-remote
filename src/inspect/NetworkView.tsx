import { useEffect, useRef, useState, useMemo } from "react";
import type { NetworkEntry } from "../types";
import { copyText } from "../utils/clipboard";

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
      /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE]+\d+)?)/g,
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
          color = "#ff453a";
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
        maxHeight: "none",
        overflowY: "auto",
        userSelect: "text",
        cursor: "text",
      }}
      dangerouslySetInnerHTML={{ __html: htmlContent }}
    />
  );
}

function getStatusBadge(status?: number, encrypted?: boolean, tlsError?: string) {
  if (tlsError) {
    return { fg: "#ff453a", bg: "rgba(255,69,58,0.3)", label: "Decrypt Failed" };
  }
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
  const host = (e.host || "").replace(/:443$/, "").replace(/:80$/, "");
  const path = e.path || "";
  if (!path) return `${scheme}://${host}`;
  return `${scheme}://${host}${path.startsWith("/") ? path : `/${path}`}`;
}

function displayUrl(e: NetworkEntry) {
  if ((e.method || "").toUpperCase() === "CONNECT") {
    return `CONNECT ${e.host || entryUrl(e)}`;
  }
  return entryUrl(e);
}

function queryParams(url: string): [string, string][] {
  try {
    const parsed = new URL(url);
    return [...parsed.searchParams.entries()];
  } catch {
    const q = url.split("?")[1];
    if (!q) return [];
    return q.split("&").map((part) => {
      const [k, ...rest] = part.split("=");
      return [decodeURIComponent(k || ""), decodeURIComponent(rest.join("=") || "")] as [string, string];
    });
  }
}

function capturedFrom(e: NetworkEntry) {
  return e.requestHeaders?.["X-Captured-From"] || "";
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

/* ---------------------------------- Copy ---------------------------------- */

function CopyButton({
  getText,
  label,
  compact,
}: {
  getText: () => string;
  label: string;
  compact?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return (
    <button
      className="surface-btn"
      title={`Copy ${label.toLowerCase()} (⌘/Ctrl+C copies selection or ${label})`}
      onClick={async (e) => {
        e.stopPropagation();
        if (await copyText(getText())) {
          setCopied(true);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1200);
        }
      }}
      style={{
        width: "auto",
        padding: compact ? "1px 6px" : "3px 8px",
        fontSize: compact ? 10 : 11,
        color: copied ? "#30d158" : "inherit",
        fontWeight: copied ? 700 : 400,
        flexShrink: 0,
      }}
    >
      {copied ? "✓ Copied" : `⧉ ${label}`}
    </button>
  );
}

function headersToText(headers?: Record<string, string>) {
  return Object.entries(headers || {})
    .map(([name, value]) => `${name}: ${value}`)
    .join("\n");
}

function buildFullDump(entry: NetworkEntry): string {
  const url = entryUrl(entry);
  const params = queryParams(url);
  const lines: string[] = [
    `${entry.method} ${url}`,
    `Status: ${entry.status ?? "—"} · ${entry.durationMs ?? "—"} ms · ${entry.size ?? "—"} B`,
    "",
  ];
  if (params.length) {
    lines.push("== Query params ==", params.map(([k, v]) => `${k}=${v}`).join("\n"), "");
  }
  lines.push("== Request headers ==", headersToText(entry.requestHeaders) || "(none)", "");
  lines.push(
    "== Request body ==",
    entry.requestBody ?? "(none)",
    "",
    "== Response headers ==",
    headersToText(entry.responseHeaders) || "(none)",
    ""
  );
  lines.push("== Response body ==", entry.responseBody ?? "(none)");
  if (entry.tlsError) lines.unshift(`TLS error: ${entry.tlsError}`, "");
  return lines.join("\n");
}

/* ------------------------------ Collapsible UI ----------------------------- */

function Section({
  id,
  title,
  openMap,
  toggle,
  actions,
  children,
}: {
  id: string;
  title: string;
  openMap: Record<string, boolean>;
  toggle: (id: string) => void;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  const open = openMap[id] !== false;
  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: 8,
        background: "rgba(255,255,255,0.015)",
      }}
    >
      <div
        onClick={() => toggle(id)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "5px 8px",
          cursor: "pointer",
          userSelect: "none",
        }}
      >
        <span style={{ color: "var(--muted)", fontSize: 10 }}>{open ? "▾" : "▸"}</span>
        <span style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", fontWeight: 700, flex: 1 }}>
          {title}
        </span>
        <div onClick={(e) => e.stopPropagation()} style={{ display: "flex", gap: 4 }}>
          {actions}
        </div>
      </div>
      {open ? (
        <div style={{ padding: "0 8px 8px" }}>{children}</div>
      ) : null}
    </div>
  );
}

function BodyViewer({ body }: { body: string }) {
  const [pretty, setPretty] = useState(true);
  const [wrap, setWrap] = useState(true);
  const isJson = useMemo(() => {
    try {
      JSON.parse(body);
      return true;
    } catch {
      return false;
    }
  }, [body]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ display: "flex", gap: 10 }}>
        {isJson ? (
          <label style={{ display: "inline-flex", gap: 4, alignItems: "center", fontSize: 10, color: "var(--muted)", cursor: "pointer" }}>
            <input type="checkbox" checked={pretty} onChange={(e) => setPretty(e.target.checked)} /> Pretty JSON
          </label>
        ) : null}
        <label style={{ display: "inline-flex", gap: 4, alignItems: "center", fontSize: 10, color: "var(--muted)", cursor: "pointer" }}>
          <input type="checkbox" checked={wrap} onChange={(e) => setWrap(e.target.checked)} /> Wrap
        </label>
      </div>
      {isJson && pretty ? (
        <JsonSyntaxViewer data={body} />
      ) : (
        <pre
          style={{
            margin: 0,
            padding: 10,
            borderRadius: 8,
            background: "#060608",
            border: "1px solid var(--border)",
            fontSize: 11,
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
            whiteSpace: wrap ? "pre-wrap" : "pre",
            wordBreak: wrap ? "break-all" : "normal",
            overflowX: wrap ? "hidden" : "auto",
            overflowY: "auto",
            maxHeight: "none",
            userSelect: "text",
            cursor: "text",
          }}
        >
          {body}
        </pre>
      )}
    </div>
  );
}

function HeaderTable({
  headers,
}: {
  headers?: Record<string, string>;
}) {
  const rows = Object.entries(headers || {});
  return (
    <div>
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
                userSelect: "text",
                cursor: "text",
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

/* ------------------------------- Main views -------------------------------- */

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
  const [hideFailedTunnels, setHideFailedTunnels] = useState(true);
  const [autoScroll, setAutoScroll] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  // Manual scroll up pauses following until the user returns to the bottom.
  const handleScroll = () => {
    const el = listRef.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  // CONNECT rows are tunnel scaffolding: hide them once real decrypted
  // requests exist for that host so the list shows the actual APIs. Keep
  // failed/opaque tunnels visible as explicit decrypt errors — unless muted.
  const filteredEntries = useMemo(() => {
    const decryptedHosts = new Set(
      (entries || [])
        .filter((e) => (e.method || "").toUpperCase() !== "CONNECT" && e.encrypted)
        .map((e) => e.host)
    );
    const q = query.toLowerCase().trim();
    return (entries || []).filter((e) => {
      if (hideFailedTunnels && e.tlsError) return false;
      if (
        (e.method || "").toUpperCase() === "CONNECT" &&
        !e.tlsError &&
        decryptedHosts.has(e.host)
      ) {
        return false;
      }
      const isFailed = (e.status && e.status >= 400) || (!e.encrypted && e.status == null);
      if (filter === "errors" && !(isFailed || e.tlsError)) return false;
      if (filter === "success" && ((e.status && e.status >= 400) || e.tlsError)) return false;

      if (q) {
        const full = `${e.method || ""} ${displayUrl(e)} ${e.host || ""} ${e.path || ""} ${e.status || ""}`.toLowerCase();
        if (!full.includes(q)) return false;
      }
      return true;
    });
  }, [entries, filter, query, hideFailedTunnels]);

  // Follow the tail while auto-scroll is enabled and the user is at the bottom.
  useEffect(() => {
    if (!autoScroll || !stickToBottom.current) return;
    const el = listRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [filteredEntries.length, autoScroll]);

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

          <label
            title="Hide rows for tunnels whose TLS could not be decrypted (system calls, pinned apps)"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              fontSize: 11,
              color: "var(--muted)",
              cursor: "pointer",
              userSelect: "none",
              whiteSpace: "nowrap",
            }}
          >
            <input
              type="checkbox"
              checked={hideFailedTunnels}
              onChange={(e) => setHideFailedTunnels(e.target.checked)}
            />
            Hide failed tunnels
          </label>

          <label
            title="Automatically scroll to the newest request"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              fontSize: 11,
              color: "var(--muted)",
              cursor: "pointer",
              userSelect: "none",
              whiteSpace: "nowrap",
            }}
          >
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => {
                setAutoScroll(e.target.checked);
                stickToBottom.current = true;
              }}
            />
            Auto-scroll
          </label>

          <button className="surface-btn" style={{ width: "auto", padding: "4px 8px", fontSize: 11 }} onClick={onClear}>
            🗑 Clear
          </button>
          <button className="primary-btn" style={{ width: "auto", padding: "4px 10px", fontSize: 11 }} onClick={onExport}>
            📥 Export HAR
          </button>
        </div>

        {/* Requests Table Body */}
        <div
          className="inspect-body"
          ref={listRef}
          onScroll={handleScroll}
          style={{ flex: 1, overflowY: "auto", padding: "4px 0" }}
        >
          {filteredEntries.map((e, idx) => {
            const badge = getStatusBadge(e.status, e.encrypted, e.tlsError);
            const methodColor = getMethodColor(e.method);
            const isFailed =
              (e.status && e.status >= 400) || (!e.encrypted && e.status == null) || !!e.tlsError;
            const isSelected = selectedId === e.id;

            return (
              <div
                key={e.id || idx}
                onClick={() => setSelectedId(isSelected ? null : e.id)}
                style={{
                  display: "grid",
                  gridTemplateColumns: "72px minmax(0, 1fr) 110px 64px 64px",
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
                  title={displayUrl(e)}
                  style={{
                    color: isFailed ? "#ff453a" : "var(--text)",
                    wordBreak: "break-all",
                    overflowWrap: "anywhere",
                    whiteSpace: "pre-wrap",
                    lineHeight: 1.4,
                    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
                    fontSize: 11,
                    minWidth: 0,
                  }}
                >
                  {displayUrl(e)}
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

      {/* Selected Request Detail Panel */}
      {selectedRow ? <RequestDetail entry={selectedRow} onClose={() => setSelectedId(null)} /> : null}
    </div>
  );
}

const DEFAULT_OPEN_SECTIONS: Record<string, boolean> = {
  overview: true,
  url: true,
  requestHeaders: false,
  requestBody: true,
  responseHeaders: false,
  responseBody: true,
};

function RequestDetail({
  entry,
  onClose,
}: {
  entry: NetworkEntry;
  onClose: () => void;
}) {
  const badge = getStatusBadge(entry.status, entry.encrypted, entry.tlsError);
  const url = entryUrl(entry);
  const params = queryParams(url);

  // Resizable / maximizable detail panel
  const [paneWidth, setPaneWidth] = useState(520);
  const [maximized, setMaximized] = useState(false);
  const dragState = useRef<{ startX: number; startWidth: number } | null>(null);

  const onMouseDownHandle = (e: React.MouseEvent) => {
    dragState.current = { startX: e.clientX, startWidth: paneWidth };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragState.current) return;
      const delta = dragState.current.startX - e.clientX;
      const next = Math.min(window.innerWidth * 0.85, Math.max(320, dragState.current.startWidth + delta));
      setPaneWidth(next);
    };
    const onUp = () => {
      if (dragState.current) {
        dragState.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  // Collapsible sections (independent, DevTools-style)
  const [openMap, setOpenMap] = useState<Record<string, boolean>>(DEFAULT_OPEN_SECTIONS);
  const toggle = (id: string) => setOpenMap((m) => ({ ...m, [id]: m[id] === false }));

  // Cmd/Ctrl+C: native selection copies itself; with no selection, copy the dump.
  const onKeyDownPane = async (e: React.KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "c") {
      const sel = window.getSelection()?.toString().trim();
      if (!sel) {
        e.preventDefault();
        await copyText(buildFullDump(entry));
      }
    }
  };

  return (
    <div
      tabIndex={0}
      onKeyDown={onKeyDownPane}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      style={{
        position: "relative",
        width: maximized ? "min(1200px, 82%)" : paneWidth,
        minWidth: 320,
        borderLeft: "1px solid var(--border)",
        background: "#0c0c0f",
        padding: 14,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        overflowY: "auto",
        fontSize: 11,
      }}
    >
      {/* Drag handle to widen/narrow the panel */}
      {!maximized ? (
        <div
          onMouseDown={onMouseDownHandle}
          title="Drag to resize"
          style={{
            position: "absolute",
            left: -3,
            top: 0,
            bottom: 0,
            width: 6,
            cursor: "col-resize",
            zIndex: 5,
          }}
        />
      ) : null}

      {/* Header */}
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
              overflowWrap: "anywhere",
              whiteSpace: "pre-wrap",
              marginTop: 4,
              lineHeight: 1.45,
              userSelect: "text",
              cursor: "text",
            }}
          >
            {url}
          </div>
          {capturedFrom(entry) ? (
            <div style={{ color: "#64d2ff", marginTop: 4 }}>{capturedFrom(entry)}</div>
          ) : null}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 4, flexShrink: 0 }}>
          <button
            className="icon-btn"
            style={{ width: 22, height: 22 }}
            title={maximized ? "Restore size" : "Maximize panel"}
            onClick={() => setMaximized((v) => !v)}
          >
            {maximized ? "⤡" : "⤢"}
          </button>
          <button className="icon-btn" style={{ width: 22, height: 22 }} onClick={onClose}>
            ×
          </button>
        </div>
      </div>

      {/* Status row */}
      <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: 6, userSelect: "none" }}>
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
        <span style={{ color: "var(--muted)" }}>
          {entry.host ? `${entry.host} · ` : ""}
          {entry.durationMs != null ? `${entry.durationMs} ms` : "—"}
          {entry.size != null ? ` · ${entry.size} B` : ""}
        </span>
        <CopyButton getText={() => url} label="URL" />
        <CopyButton getText={() => buildFullDump(entry)} label="All" />
      </div>

      {entry.tlsError ? (
        <div
          style={{
            color: "#ff453a",
            background: "rgba(255,69,58,0.12)",
            border: "1px solid rgba(255,69,58,0.4)",
            borderRadius: 8,
            padding: "6px 10px",
            wordBreak: "break-word",
            lineHeight: 1.45,
            userSelect: "text",
          }}
        >
          TLS decrypt failed — traffic not captured.
          {entry.tlsError.includes("certificate_unknown") || entry.tlsError.includes("CertificateUnknown")
            ? " The TV rejected our MITM certificate (certificate_unknown): the app is certificate-pinned, or the MITM CA is not trusted by this build."
            : ""}
        </div>
      ) : null}

      {/* Collapsible sections */}
      <Section
        id="overview"
        title="Overview"
        openMap={openMap}
        toggle={toggle}
      >
        <HeaderTable
          headers={{
            Method: entry.method,
            Host: entry.host || "—",
            Path: entry.path || "—",
            Status: entry.status != null ? String(entry.status) : "—",
            Duration: entry.durationMs != null ? `${entry.durationMs} ms` : "—",
            Size: entry.size != null ? `${entry.size} B` : "—",
            Scheme: entry.encrypted ? "https" : "http",
            ...(capturedFrom(entry) ? { Source: capturedFrom(entry) } : {}),
          }}
        />
      </Section>

      <Section
        id="url"
        title={`URL + query (${params.length})`}
        openMap={openMap}
        toggle={toggle}
        actions={<CopyButton compact getText={() => url} label="URL" />}
      >
        <div style={{ marginBottom: 6 }}>
          <div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 4 }}>Full URL</div>
          <div
            style={{
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
              wordBreak: "break-all",
              lineHeight: 1.45,
              userSelect: "text",
              cursor: "text",
              padding: "4px 8px",
              background: "#060608",
              border: "1px solid var(--border)",
              borderRadius: 8,
            }}
          >
            {url}
          </div>
        </div>
        {params.length > 0 ? <HeaderTable headers={Object.fromEntries(params)} /> : null}
      </Section>

      <Section
        id="requestHeaders"
        title="Request headers"
        openMap={openMap}
        toggle={toggle}
        actions={
          <CopyButton compact getText={() => headersToText(entry.requestHeaders)} label="Request headers" />
        }
      >
        <HeaderTable headers={entry.requestHeaders} />
      </Section>

      <Section
        id="requestBody"
        title="Request body"
        openMap={openMap}
        toggle={toggle}
        actions={
          entry.requestBody ? <CopyButton compact getText={() => entry.requestBody || ""} label="Request body" /> : null
        }
      >
        {entry.requestBody ? (
          <BodyViewer body={entry.requestBody} />
        ) : (
          <div style={{ color: "var(--muted)" }}>
            {entry.tlsError
              ? "TLS tunnel could not be decrypted."
              : (entry.method || "").toUpperCase() === "CONNECT"
              ? "Tunnel row — open a decrypted request below the same host for path/body."
              : "No request body."}
          </div>
        )}
      </Section>

      <Section
        id="responseHeaders"
        title="Response headers"
        openMap={openMap}
        toggle={toggle}
        actions={
          <CopyButton compact getText={() => headersToText(entry.responseHeaders)} label="Response headers" />
        }
      >
        <HeaderTable headers={entry.responseHeaders} />
      </Section>

      <Section
        id="responseBody"
        title="Response body"
        openMap={openMap}
        toggle={toggle}
        actions={
          entry.responseBody ? (
            <CopyButton compact getText={() => entry.responseBody || ""} label="Response body" />
          ) : null
        }
      >
        {entry.responseBody ? (
          <BodyViewer body={entry.responseBody} />
        ) : (
          <div style={{ color: "var(--muted)" }}>
            {entry.tlsError
              ? "TLS tunnel could not be decrypted."
              : (entry.method || "").toUpperCase() === "CONNECT"
              ? "Tunnel row — open a decrypted request below the same host."
              : "No response body."}
          </div>
        )}
      </Section>
    </div>
  );
}
