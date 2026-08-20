import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { LogLevel, LogLine } from "../types";

const LEVELS: LogLevel[] = ["V", "D", "I", "W", "E", "F"];

const LEVEL_COLORS: Record<string, { fg: string; bg: string; border: string }> = {
  V: { fg: "#8e8e93", bg: "rgba(142,142,147,0.12)", border: "rgba(142,142,147,0.3)" },
  D: { fg: "#64d2ff", bg: "rgba(100,210,255,0.14)", border: "rgba(100,210,255,0.35)" },
  I: { fg: "#30d158", bg: "rgba(48,209,88,0.14)", border: "rgba(48,209,88,0.35)" },
  W: { fg: "#ffd60a", bg: "rgba(255,214,10,0.18)", border: "rgba(255,214,10,0.4)" },
  E: { fg: "#ff453a", bg: "rgba(255,69,58,0.22)", border: "rgba(255,69,58,0.45)" },
  F: { fg: "#bf5af2", bg: "rgba(191,90,242,0.28)", border: "rgba(191,90,242,0.5)" },
};

function getTagColor(tag: string) {
  let hash = 0;
  for (let i = 0; i < tag.length; i++) {
    hash = (hash << 5) - hash + tag.charCodeAt(i);
    hash |= 0;
  }
  const hue = Math.abs(hash) % 360;
  return `hsl(${hue}, 70%, 65%)`;
}

export function LogcatView({
  lines,
  paused,
  level,
  query,
  tag,
  pkg,
  onLevel,
  onQuery,
  onTag,
  onPkg,
  onPause,
  onClear,
  onExport,
}: {
  lines: LogLine[];
  paused: boolean;
  level: LogLevel;
  query: string;
  tag: string;
  pkg: string;
  onLevel: (l: LogLevel) => void;
  onQuery: (q: string) => void;
  onTag: (t: string) => void;
  onPkg: (p: string) => void;
  onPause: () => void;
  onClear: () => void;
  onExport: () => void;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [selectedLine, setSelectedLine] = useState<LogLine | null>(null);

  const filtered = useMemo(() => {
    const min = LEVELS.indexOf(level);
    const q = (query || "").toLowerCase();
    const t = (tag || "").toLowerCase();
    const p = (pkg || "").toLowerCase();
    return (lines || []).filter((l) => {
      if (!l || !l.level) return false;
      if (LEVELS.indexOf(l.level as LogLevel) < min) return false;
      if (t && (!l.tag || !l.tag.toLowerCase().includes(t))) return false;
      const hay = `${l.tag || ""} ${l.message || ""}`.toLowerCase();
      if (q && !hay.includes(q)) return false;
      if (p && !hay.includes(p)) return false;
      return true;
    });
  }, [lines, level, query, tag, pkg]);

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 22,
    overscan: 25,
  });

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      {/* Colorful Control Toolbar */}
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
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "var(--muted)" }}>Min:</span>
          <select
            value={level}
            onChange={(e) => onLevel(e.target.value as LogLevel)}
            style={{
              background: "#16161a",
              color: LEVEL_COLORS[level]?.fg || "#fff",
              fontWeight: 700,
              border: "1px solid var(--border)",
            }}
          >
            {LEVELS.map((l) => (
              <option key={l} value={l}>
                {l}+ {l === "V" ? "Verbose" : l === "D" ? "Debug" : l === "I" ? "Info" : l === "W" ? "Warn" : l === "E" ? "Error" : "Fatal"}
              </option>
            ))}
          </select>
        </div>

        <input
          placeholder="Filter Tag…"
          value={tag || ""}
          onChange={(e) => onTag(e.target.value)}
          style={{ width: 110 }}
        />
        <input
          placeholder="Search logs…"
          value={query || ""}
          onChange={(e) => onQuery(e.target.value)}
          style={{ flex: 1, minWidth: 140 }}
        />

        <select value={pkg || ""} onChange={(e) => onPkg(e.target.value)}>
          <option value="">All Packages</option>
          <option value="com.intigral.jawwytv">STC TV</option>
          <option value="net.intigral.jawwytv">Jawwy</option>
        </select>

        <button
          className="surface-btn"
          style={{
            width: "auto",
            padding: "5px 10px",
            fontSize: 12,
            background: paused ? "rgba(255,214,10,0.2)" : "rgba(255,255,255,0.06)",
            color: paused ? "#ffd60a" : "inherit",
          }}
          onClick={onPause}
        >
          {paused ? "▶ Resume" : "⏸ Pause"}
        </button>
        <button
          className="surface-btn"
          style={{ width: "auto", padding: "5px 10px", fontSize: 12 }}
          onClick={onClear}
        >
          🗑 Clear
        </button>
        <button
          className="primary-btn"
          style={{ width: "auto", padding: "5px 12px", fontSize: 12 }}
          onClick={onExport}
        >
          📥 Export Log
        </button>
      </div>

      {/* Main Colorful Virtualized Log Table & Inspector */}
      <div style={{ display: "flex", flex: 1, minHeight: 0, position: "relative" }}>
        <div className="inspect-body" ref={parentRef} style={{ flex: 1, minWidth: 0, padding: "4px 0" }}>
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((v) => {
              const l = filtered[v.index];
              if (!l) return null;
              const lvlStyle = LEVEL_COLORS[l.level] || LEVEL_COLORS.I;
              const tagColor = getTagColor(l.tag || "sys");
              const isSelected = selectedLine?.id === l.id;

              return (
                <div
                  key={l.id ?? v.index}
                  onClick={() => setSelectedLine(isSelected ? null : l)}
                  className="log-line"
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${v.start}px)`,
                    height: 22,
                    cursor: "pointer",
                    background: isSelected
                      ? "rgba(10, 132, 255, 0.25)"
                      : v.index % 2 === 0
                      ? "rgba(255,255,255,0.015)"
                      : "transparent",
                    borderLeft: isSelected ? "3px solid #0a84ff" : "3px solid transparent",
                  }}
                >
                  <span style={{ fontSize: 11, color: "var(--muted)", fontVariantNumeric: "tabular-nums" }}>
                    {l.time || "—"}
                  </span>
                  <span
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      justifyContent: "center",
                      fontSize: 10,
                      fontWeight: 800,
                      color: lvlStyle.fg,
                      background: lvlStyle.bg,
                      borderRadius: 4,
                      padding: "1px 4px",
                      lineHeight: 1,
                    }}
                  >
                    {l.level || "I"}
                  </span>
                  <span
                    style={{
                      color: tagColor,
                      fontWeight: 600,
                      fontSize: 11,
                    }}
                  >
                    {l.tag || "system"}
                  </span>
                  <span style={{ color: lvlStyle.fg, fontSize: 11 }}>
                    {l.message || ""}
                  </span>
                </div>
              );
            })}
          </div>
        </div>

        {/* Selected Log Line Detail Inspector Box */}
        {selectedLine ? (
          <div
            style={{
              width: 320,
              borderLeft: "1px solid var(--border)",
              background: "#111115",
              padding: 14,
              display: "flex",
              flexDirection: "column",
              gap: 10,
              overflowY: "auto",
              fontSize: 12,
            }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  color: LEVEL_COLORS[selectedLine.level]?.fg || "#fff",
                  background: LEVEL_COLORS[selectedLine.level]?.bg,
                  padding: "2px 8px",
                  borderRadius: 6,
                }}
              >
                Level: {selectedLine.level}
              </span>
              <button
                className="icon-btn"
                style={{ width: 22, height: 22, fontSize: 12 }}
                onClick={() => setSelectedLine(null)}
              >
                ×
              </button>
            </div>

            <div>
              <div style={{ fontSize: 10, color: "var(--muted)", textTransform: "uppercase" }}>Tag</div>
              <div style={{ color: getTagColor(selectedLine.tag), fontWeight: 700, fontSize: 13 }}>
                {selectedLine.tag || "—"}
              </div>
            </div>

            <div>
              <div style={{ fontSize: 10, color: "var(--muted)", textTransform: "uppercase" }}>Time & Process</div>
              <div style={{ color: "var(--text)", fontSize: 11, fontVariantNumeric: "tabular-nums" }}>
                {selectedLine.time || "—"} · PID: {selectedLine.pid || "—"} · TID: {selectedLine.tid || "—"}
              </div>
            </div>

            <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
              <div style={{ fontSize: 10, color: "var(--muted)", textTransform: "uppercase", marginBottom: 4 }}>
                Message
              </div>
              <textarea
                readOnly
                value={selectedLine.message}
                style={{
                  flex: 1,
                  minHeight: 120,
                  background: "rgba(0,0,0,0.5)",
                  color: LEVEL_COLORS[selectedLine.level]?.fg || "#fff",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 8,
                  fontSize: 11,
                  fontFamily: "monospace",
                  resize: "none",
                }}
              />
            </div>

            <button
              className="surface-btn"
              style={{ fontSize: 12, padding: "6px 10px" }}
              onClick={() => {
                navigator.clipboard.writeText(
                  `[${selectedLine.time}] ${selectedLine.level}/${selectedLine.tag}: ${selectedLine.message}`
                );
              }}
            >
              📋 Copy Log Line
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
