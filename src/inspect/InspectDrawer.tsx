import { useState, useRef, useEffect } from "react";
import type { CrashEntry, LogLevel, LogLine, NetworkEntry } from "../types";
import { CrashesView } from "./CrashesView";
import { LogcatView } from "./LogcatView";
import { NetworkView } from "./NetworkView";

export type InspectTab = "logs" | "network" | "crashes";

export function InspectDrawer({
  tab,
  onTab,
  onClose,
  logs,
  network,
  crashes,
}: {
  tab: InspectTab;
  onTab: (t: InspectTab) => void;
  onClose: () => void;
  logs: {
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
  };
  network: {
    entries: NetworkEntry[];
    onClear: () => void;
    onExport: () => void;
  };
  crashes: {
    entries: CrashEntry[];
    onSave: (id: string) => void;
  };
}) {
  const [height, setHeight] = useState(360);
  const [isMaximized, setIsMaximized] = useState(false);
  const isDragging = useRef(false);
  const startY = useRef(0);
  const startHeight = useRef(360);

  const onMouseDown = (e: React.MouseEvent) => {
    isDragging.current = true;
    startY.current = e.clientY;
    startHeight.current = height;
    document.body.style.cursor = "row-resize";
    document.body.style.userSelect = "none";
  };

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      const deltaY = startY.current - e.clientY;
      const newHeight = Math.max(200, Math.min(window.innerHeight * 0.85, startHeight.current + deltaY));
      setHeight(newHeight);
    };

    const onMouseUp = () => {
      if (isDragging.current) {
        isDragging.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  const currentHeight = isMaximized ? "75vh" : `${height}px`;

  return (
    <div
      className="inspect"
      style={{
        height: currentHeight,
        transition: isDragging.current ? "none" : "height 0.15s ease",
        position: "relative",
        display: "flex",
        flexDirection: "column",
        background: "#0c0c0e",
        borderTop: "1px solid var(--border)",
      }}
    >
      {/* Resizable Top Drag Handle Bar */}
      <div
        onMouseDown={onMouseDown}
        style={{
          height: 6,
          width: "100%",
          cursor: "row-resize",
          background: "transparent",
          position: "absolute",
          top: 0,
          left: 0,
          zIndex: 50,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
        title="Drag up/down to resize log panel"
      >
        <div
          style={{
            width: 36,
            height: 3,
            borderRadius: 2,
            background: "rgba(255,255,255,0.25)",
          }}
        />
      </div>

      <div className="inspect-tabs" style={{ paddingTop: 8 }}>
        {(["logs", "network", "crashes"] as InspectTab[]).map((t) => {
          const count =
            t === "logs"
              ? logs.lines.length
              : t === "network"
              ? network.entries.length
              : crashes.entries.length;
          return (
            <button
              key={t}
              className={`inspect-tab${tab === t ? " active" : ""}`}
              onClick={() => onTab(t)}
              style={{ display: "inline-flex", alignItems: "center", gap: 6 }}
            >
              <span>{t === "logs" ? "Logs" : t === "network" ? "Network" : "Crashes"}</span>
              <span
                style={{
                  fontSize: 10,
                  padding: "1px 6px",
                  borderRadius: 999,
                  background: tab === t ? "rgba(10,132,255,0.25)" : "rgba(255,255,255,0.06)",
                  color: tab === t ? "#0a84ff" : "var(--muted)",
                  fontWeight: 600,
                }}
              >
                {count}
              </span>
            </button>
          );
        })}

        <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 6 }}>
          <button
            className="icon-btn"
            style={{ width: 28, height: 28, fontSize: 13 }}
            title={isMaximized ? "Restore Height" : "Maximize Drawer"}
            onClick={() => setIsMaximized((v) => !v)}
          >
            {isMaximized ? "⤡" : "⤢"}
          </button>
          <button
            className="icon-btn"
            style={{ width: 28, height: 28 }}
            onClick={onClose}
          >
            ×
          </button>
        </div>
      </div>

      {tab === "logs" ? <LogcatView {...logs} /> : null}
      {tab === "network" ? <NetworkView {...network} /> : null}
      {tab === "crashes" ? <CrashesView {...crashes} /> : null}
    </div>
  );
}
