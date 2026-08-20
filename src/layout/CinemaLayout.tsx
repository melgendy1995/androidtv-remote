import type { ReactNode } from "react";

export function CinemaLayout({
  sidebarHidden,
  inspectOpen,
  stage,
  sidebar,
  inspect,
  onShowSidebar,
}: {
  sidebarHidden: boolean;
  inspectOpen: boolean;
  stage: ReactNode;
  sidebar: ReactNode;
  inspect: ReactNode;
  onShowSidebar: () => void;
}) {
  return (
    <div className="cinema" style={{ display: "flex", width: "100vw", height: "100vh", overflow: "hidden" }}>
      <div className="stage-column" style={{ flex: 1, minWidth: 0, height: "100%", position: "relative", display: "flex", flexDirection: "column" }}>
        {sidebarHidden && (
          <button
            className="show-remote glass"
            title="Show Controls & Remote"
            onClick={onShowSidebar}
            style={{
              position: "absolute",
              top: 16,
              right: 16,
              zIndex: 100,
              background: "#0a84ff",
              color: "#ffffff",
              padding: "8px 14px",
              borderRadius: 8,
              fontWeight: 600,
              fontSize: "13px",
              border: "none",
              cursor: "pointer",
              boxShadow: "0 4px 12px rgba(0,0,0,0.5)",
            }}
          >
            « Show Controls & Remote
          </button>
        )}
        {stage}
        {inspectOpen ? inspect : null}
      </div>
      {!sidebarHidden && <aside className="sidebar">{sidebar}</aside>}
    </div>
  );
}
