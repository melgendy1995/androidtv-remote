import { useCallback, useEffect, useMemo, useState } from "react";
import { desktopDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { AppInfo } from "../types";

type Props = {
  open: boolean;
  onClose: () => void;
};

const STREAMING_KEYWORDS = [
  "tv",
  "video",
  "stream",
  "jawwy",
  "intigral",
  "shahid",
  "netflix",
  "youtube",
  "amazon",
  "disney",
  "apple",
  "spotify",
  "plex",
  "kodi",
  "hulu",
  "hbomax",
  "prime",
  "cinema",
  "iptv",
  "player",
];

function isStreamingApp(app: AppInfo): boolean {
  const name = `${app.label} ${app.packageName}`.toLowerCase();
  return STREAMING_KEYWORDS.some((kw) => name.includes(kw));
}

function getAppGradient(pkg: string): string {
  let hash = 0;
  for (let i = 0; i < pkg.length; i++) {
    hash = (hash << 5) - hash + pkg.charCodeAt(i);
    hash |= 0;
  }
  const hue1 = Math.abs(hash) % 360;
  const hue2 = (hue1 + 40) % 360;
  return `linear-gradient(135deg, hsl(${hue1}, 75%, 28%), hsl(${hue2}, 85%, 18%))`;
}

export function AppDrawer({ open, onClose }: Props) {
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<"all" | "streaming" | "user" | "system">("all");
  const [loading, setLoading] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [actionStatus, setActionStatus] = useState<string>();
  const [apkPath, setApkPath] = useState("");

  const loadApps = useCallback(async () => {
    setLoading(true);
    setActionError(undefined);
    try {
      const data = await api.listApps();
      setApps(data);
    } catch (e) {
      setActionError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) {
      loadApps();
    }
  }, [open, loadApps]);

  const { featuredApps, regularApps } = useMemo(() => {
    const q = query.toLowerCase().trim();
    const sorted = [...(apps || [])].sort((a, b) => {
      // User apps first
      if (a.isSystem !== b.isSystem) return a.isSystem ? 1 : -1;
      // Then alphabetical
      return a.label.localeCompare(b.label);
    });

    const filtered = sorted.filter((app) => {
      if (category === "user" && app.isSystem) return false;
      if (category === "system" && !app.isSystem) return false;
      if (category === "streaming" && !isStreamingApp(app)) return false;

      if (q) {
        return (
          app.label.toLowerCase().includes(q) ||
          app.packageName.toLowerCase().includes(q)
        );
      }
      return true;
    });

    // Extract top streaming & user apps as Featured
    const featured = filtered.filter((a) => !a.isSystem && isStreamingApp(a)).slice(0, 8);
    const featuredPkgs = new Set(featured.map((f) => f.packageName));
    const regular = filtered.filter((a) => !featuredPkgs.has(a.packageName));

    return { featuredApps: featured, regularApps: regular };
  }, [apps, query, category]);

  if (!open) return null;

  const handleLaunch = async (pkg: string) => {
    setActionError(undefined);
    setActionStatus(`Launching ${pkg}...`);
    try {
      await api.launchApp(pkg);
      setActionStatus(`Launched!`);
      setTimeout(() => setActionStatus(undefined), 2000);
    } catch (e) {
      setActionError(String(e));
      setActionStatus(undefined);
    }
  };

  const handleForceStop = async (pkg: string) => {
    setActionError(undefined);
    setActionStatus(`Force stopping ${pkg}...`);
    try {
      await api.forceStopApp(pkg);
      setActionStatus(`Stopped ${pkg}`);
      setTimeout(() => setActionStatus(undefined), 2000);
    } catch (e) {
      setActionError(String(e));
      setActionStatus(undefined);
    }
  };

  const handleUninstall = async (pkg: string) => {
    if (!confirm(`Uninstall ${pkg}?`)) return;
    setActionError(undefined);
    setActionStatus(`Uninstalling ${pkg}...`);
    try {
      await api.uninstallApp(pkg);
      setActionStatus(`Uninstalled ${pkg}`);
      loadApps();
      setTimeout(() => setActionStatus(undefined), 2000);
    } catch (e) {
      setActionError(String(e));
      setActionStatus(undefined);
    }
  };

  const handleBrowseApk = async () => {
    setActionError(undefined);
    setActionStatus(undefined);
    try {
      let defaultPath: string | undefined;
      try {
        defaultPath = await desktopDir();
      } catch {
        defaultPath = undefined;
      }
      const selected = await open({
        multiple: false,
        directory: false,
        defaultPath,
        title: "Select APK",
        filters: [{ name: "Android package", extensions: ["apk"] }],
      });
      if (typeof selected === "string" && selected) {
        setApkPath(selected);
      }
    } catch (e) {
      setActionError(String(e));
    }
  };

  const handleInstallApk = async () => {
    if (!apkPath.trim()) return;
    setActionError(undefined);
    setActionStatus("Installing APK...");
    try {
      const res = await api.installApk(apkPath.trim());
      setActionStatus(res);
      setApkPath("");
      loadApps();
      setTimeout(() => setActionStatus(undefined), 3000);
    } catch (e) {
      setActionError(String(e));
      setActionStatus(undefined);
    }
  };

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div
        className="sheet"
        onClick={(e) => e.stopPropagation()}
        style={{
          width: "min(720px, calc(100vw - 32px))",
          maxHeight: "85vh",
          display: "flex",
          flexDirection: "column",
          padding: 0,
          background: "#0d0d11",
          border: "1px solid var(--border)",
          overflow: "hidden",
        }}
      >
        {/* Top Header */}
        <div
          style={{
            padding: "14px 18px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            background: "#131318",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 20 }}>🚀</span>
            <div>
              <h2 style={{ margin: 0, fontSize: 16, fontWeight: 700 }}>App Launcher & Sideloading</h2>
              <div style={{ fontSize: 11, color: "var(--muted)" }}>
                Launch, manage, or install APKs on your Android TV
              </div>
            </div>
          </div>
          <button className="icon-btn" style={{ width: 28, height: 28 }} onClick={onClose}>
            ×
          </button>
        </div>

        {/* Action / Status Banners */}
        {actionError && (
          <div style={{ padding: "8px 16px", background: "rgba(255,69,58,0.2)", color: "#ff453a", fontSize: 12, borderBottom: "1px solid var(--border)" }}>
            ⚠️ {actionError}
          </div>
        )}
        {actionStatus && (
          <div style={{ padding: "8px 16px", background: "rgba(48,209,88,0.2)", color: "#30d158", fontSize: 12, borderBottom: "1px solid var(--border)", fontWeight: 600 }}>
            ✓ {actionStatus}
          </div>
        )}

        {/* APK Sideloading Toolbar */}
        <div
          style={{
            padding: "10px 16px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            gap: 8,
            alignItems: "center",
            background: "rgba(255,255,255,0.02)",
          }}
        >
          <input
            type="text"
            placeholder="Selected APK path (or paste path)..."
            value={apkPath}
            onChange={(e) => setApkPath(e.target.value)}
            style={{
              flex: 1,
              padding: "8px 12px",
              borderRadius: 8,
              border: "1px solid var(--border)",
              background: "#16161c",
              color: "inherit",
              fontSize: 12,
            }}
          />
          <button
            className="surface-btn"
            onClick={handleBrowseApk}
            style={{ width: "auto", padding: "8px 12px", fontSize: 12 }}
          >
            📂 Browse APK…
          </button>
          <button
            className="primary-btn"
            onClick={handleInstallApk}
            disabled={!apkPath.trim() || loading}
            style={{ width: "auto", padding: "8px 16px", fontSize: 12 }}
          >
            📦 Sideload APK
          </button>
        </div>

        {/* Search & Category Filter Chips */}
        <div
          style={{
            padding: "10px 16px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            gap: 8,
            alignItems: "center",
            flexWrap: "wrap",
            background: "#111115",
          }}
        >
          <input
            type="text"
            placeholder="Search installed apps..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            style={{
              flex: 1,
              minWidth: 160,
              padding: "6px 12px",
              borderRadius: 8,
              border: "1px solid var(--border)",
              background: "#16161c",
              color: "inherit",
              fontSize: 12,
            }}
          />

          <div style={{ display: "flex", gap: 4 }}>
            {(
              [
                { id: "all", label: "All" },
                { id: "streaming", label: "📺 TV & Media" },
                { id: "user", label: "👤 User Apps" },
                { id: "system", label: "⚙ System" },
              ] as const
            ).map((cat) => (
              <button
                key={cat.id}
                className="surface-btn"
                onClick={() => setCategory(cat.id)}
                style={{
                  width: "auto",
                  padding: "5px 10px",
                  fontSize: 11,
                  background: category === cat.id ? "rgba(10,132,255,0.25)" : "rgba(255,255,255,0.05)",
                  color: category === cat.id ? "#0a84ff" : "inherit",
                  fontWeight: category === cat.id ? 600 : 400,
                  borderRadius: 6,
                }}
              >
                {cat.label}
              </button>
            ))}
          </div>

          <button
            className="surface-btn"
            onClick={loadApps}
            disabled={loading}
            style={{ width: "auto", padding: "5px 10px", fontSize: 11 }}
          >
            {loading ? "Refreshing..." : "↻ Refresh"}
          </button>
        </div>

        {/* Main App Grid Area */}
        <div style={{ flex: 1, overflowY: "auto", padding: 16, display: "flex", flexDirection: "column", gap: 18 }}>
          {/* Section 1: Featured TV & Media Apps */}
          {featuredApps.length > 0 && !query && category !== "system" && (
            <div>
              <div style={{ fontSize: 11, fontWeight: 700, color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 10 }}>
                ⭐ Featured TV & Media Apps
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 10 }}>
                {featuredApps.map((app) => (
                  <div
                    key={app.packageName}
                    onClick={() => handleLaunch(app.packageName)}
                    style={{
                      background: getAppGradient(app.packageName),
                      padding: 12,
                      borderRadius: 12,
                      cursor: "pointer",
                      border: "1px solid rgba(255,255,255,0.15)",
                      display: "flex",
                      flexDirection: "column",
                      justifyContent: "space-between",
                      height: 80,
                      boxShadow: "0 4px 12px rgba(0,0,0,0.4)",
                      transition: "transform 0.15s ease",
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.transform = "scale(1.03)")}
                    onMouseLeave={(e) => (e.currentTarget.style.transform = "scale(1)")}
                  >
                    <div style={{ fontWeight: 700, fontSize: 13, color: "#fff", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {app.label}
                    </div>
                    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                      <span style={{ fontSize: 10, color: "rgba(255,255,255,0.7)" }}>Launch</span>
                      <span style={{ fontSize: 12 }}>▶</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Section 2: All Installed Apps */}
          <div>
            <div style={{ fontSize: 11, fontWeight: 700, color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 10 }}>
              {category === "streaming"
                ? "📺 Streaming & TV Applications"
                : category === "system"
                ? "⚙ System Packages"
                : "📱 Installed Applications"} ({regularApps.length})
            </div>

            {regularApps.length === 0 ? (
              <div style={{ textAlign: "center", opacity: 0.6, padding: 32, fontSize: 13 }}>
                {loading ? "Loading installed apps..." : "No matching applications found."}
              </div>
            ) : (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(210px, 1fr))", gap: 10 }}>
                {regularApps.map((app) => (
                  <div
                    key={app.packageName}
                    style={{
                      padding: 12,
                      borderRadius: 10,
                      border: "1px solid var(--border)",
                      background: "#16161c",
                      display: "flex",
                      flexDirection: "column",
                      justifyContent: "space-between",
                      gap: 8,
                    }}
                  >
                    <div>
                      <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text)", wordBreak: "break-word" }}>
                        {app.label}
                      </div>
                      <div style={{ fontSize: 10, color: "var(--muted)", wordBreak: "break-all", marginTop: 2 }}>
                        {app.packageName}
                      </div>
                      {app.isSystem && (
                        <span style={{ fontSize: 9, padding: "2px 6px", background: "rgba(255,255,255,0.08)", color: "var(--muted)", borderRadius: 4, display: "inline-block", marginTop: 4 }}>
                          System
                        </span>
                      )}
                    </div>

                    <div style={{ display: "flex", gap: 4, marginTop: 4 }}>
                      <button
                        className="primary-btn"
                        onClick={() => handleLaunch(app.packageName)}
                        style={{ flex: 1, padding: "5px 8px", fontSize: 11 }}
                      >
                        ▶ Launch
                      </button>
                      <button
                        className="surface-btn"
                        onClick={() => handleForceStop(app.packageName)}
                        title="Force Stop App"
                        style={{ width: "auto", padding: "5px 8px", fontSize: 11 }}
                      >
                        ⏹ Stop
                      </button>
                      {!app.isSystem && (
                        <button
                          className="danger-btn"
                          onClick={() => handleUninstall(app.packageName)}
                          title="Uninstall App"
                          style={{ width: "auto", padding: "5px 8px", fontSize: 11 }}
                        >
                          🗑
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
