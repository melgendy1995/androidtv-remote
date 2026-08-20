import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { RemoteFile } from "../types";

type Props = {
  open: boolean;
  onClose: () => void;
};

function joinLocal(folder: string, name: string) {
  const trimmed = folder.trim();
  if (!trimmed) return name;
  const sep = trimmed.includes("\\") ? "\\" : "/";
  if (trimmed.endsWith("/") || trimmed.endsWith("\\")) {
    return `${trimmed}${name}`;
  }
  return `${trimmed}${sep}${name}`;
}

export function FileManagerDrawer({ open, onClose }: Props) {
  const [currentPath, setCurrentPath] = useState("/sdcard");
  const [pathDraft, setPathDraft] = useState("/sdcard");
  const [files, setFiles] = useState<RemoteFile[]>([]);
  const [defaultPullDir, setDefaultPullDir] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [statusMsg, setStatusMsg] = useState<string>();
  const [newFolderName, setNewFolderName] = useState("");
  const [showMkdir, setShowMkdir] = useState(false);
  const [localPushPath, setLocalPushPath] = useState("");
  const [localPullPath, setLocalPullPath] = useState("");

  const loadFiles = useCallback(async (path: string) => {
    setLoading(true);
    setError(undefined);
    try {
      const res = await api.listFiles(path);
      setFiles(res);
      setCurrentPath(path);
      setPathDraft(path);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    loadFiles(currentPath);
    api
      .getSettings()
      .then((s) => setDefaultPullDir(s.captureDir || ""))
      .catch(() => undefined);
  }, [open, loadFiles]);

  if (!open) return null;

  const navigateUp = () => {
    if (currentPath === "/" || currentPath === "") return;
    const parts = currentPath.split("/").filter(Boolean);
    parts.pop();
    const parent = "/" + parts.join("/");
    loadFiles(parent === "" ? "/" : parent);
  };

  const handleOpenItem = (file: RemoteFile) => {
    if (file.isDir) {
      loadFiles(file.path);
    }
  };

  const handleDelete = async (file: RemoteFile) => {
    if (!confirm(`Delete ${file.name}?`)) return;
    setError(undefined);
    setStatusMsg(`Deleting ${file.name}...`);
    try {
      await api.deleteFile(file.path);
      setStatusMsg("Deleted");
      loadFiles(currentPath);
      setTimeout(() => setStatusMsg(undefined), 2000);
    } catch (e) {
      setError(String(e));
      setStatusMsg(undefined);
    }
  };

  const handleMkdir = async () => {
    if (!newFolderName.trim()) return;
    const path = currentPath.endsWith("/")
      ? `${currentPath}${newFolderName.trim()}`
      : `${currentPath}/${newFolderName.trim()}`;
    setError(undefined);
    try {
      await api.mkdirRemote(path);
      setNewFolderName("");
      setShowMkdir(false);
      loadFiles(currentPath);
    } catch (e) {
      setError(String(e));
    }
  };

  const handlePush = async () => {
    if (!localPushPath.trim()) return;
    setError(undefined);
    setStatusMsg("Uploading file via ADB...");
    try {
      await api.pushFile(localPushPath.trim(), currentPath);
      setStatusMsg("File uploaded successfully!");
      setLocalPushPath("");
      loadFiles(currentPath);
      setTimeout(() => setStatusMsg(undefined), 2000);
    } catch (e) {
      setError(String(e));
      setStatusMsg(undefined);
    }
  };

  const handlePull = async (file: RemoteFile) => {
    const folder = localPullPath.trim() || defaultPullDir;
    const dest = joinLocal(folder, file.name);
    setError(undefined);
    setStatusMsg(`Downloading ${file.name} to ${dest}...`);
    try {
      await api.pullFile(file.path, dest);
      setStatusMsg(`Downloaded to ${dest}`);
      setTimeout(() => setStatusMsg(undefined), 3000);
    } catch (e) {
      setError(String(e));
      setStatusMsg(undefined);
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return "-";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div
        className="sheet-container file-manager-sheet"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 750, width: "90vw", maxHeight: "85vh", display: "flex", flexDirection: "column" }}
      >
        <div className="sheet-header">
          <h2>ADB File Explorer</h2>
          <button className="icon-button" onClick={onClose}>
            ✕
          </button>
        </div>

        {error && <div className="sheet-error">{error}</div>}
        {statusMsg && <div className="sheet-success">{statusMsg}</div>}

        {/* Path Navigation Bar */}
        <div style={{ padding: "8px 16px", borderBottom: "1px solid var(--border-color)", display: "flex", gap: 8, alignItems: "center" }}>
          <button className="button button-secondary button-sm" onClick={navigateUp} disabled={currentPath === "/"}>
            ⬆ Up
          </button>
          <input
            type="text"
            value={pathDraft}
            onChange={(e) => setPathDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && loadFiles(pathDraft)}
            style={{ flex: 1, padding: "6px 10px", borderRadius: 4, border: "1px solid var(--border-color)", background: "var(--bg-input)", color: "inherit", fontFamily: "monospace" }}
          />
          <button className="button button-secondary button-sm" onClick={() => loadFiles(pathDraft)} disabled={loading}>
            Go
          </button>
          <button className="button button-primary button-sm" onClick={() => setShowMkdir(!showMkdir)}>
            + Folder
          </button>
        </div>

        {/* Actions Bar */}
        <div style={{ padding: "8px 16px", borderBottom: "1px solid var(--border-color)", display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <input
            type="text"
            placeholder="Local desktop file path to upload..."
            value={localPushPath}
            onChange={(e) => setLocalPushPath(e.target.value)}
            style={{ flex: 1, minWidth: 200, padding: "4px 8px", borderRadius: 4, border: "1px solid var(--border-color)", background: "var(--bg-input)", color: "inherit", fontSize: "0.85rem" }}
          />
          <button className="button button-secondary button-sm" onClick={handlePush}>
            Upload to TV
          </button>
          <input
            type="text"
            placeholder={defaultPullDir || "Local download folder"}
            value={localPullPath}
            onChange={(e) => setLocalPullPath(e.target.value)}
            style={{ width: 180, padding: "4px 8px", borderRadius: 4, border: "1px solid var(--border-color)", background: "var(--bg-input)", color: "inherit", fontSize: "0.85rem" }}
          />
        </div>

        {showMkdir && (
          <div style={{ padding: "8px 16px", background: "rgba(255,255,255,0.05)", display: "flex", gap: 8 }}>
            <input
              type="text"
              placeholder="New folder name..."
              value={newFolderName}
              onChange={(e) => setNewFolderName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleMkdir()}
              style={{ flex: 1, padding: "4px 8px", borderRadius: 4, border: "1px solid var(--border-color)", background: "var(--bg-input)", color: "inherit" }}
            />
            <button className="button button-primary button-sm" onClick={handleMkdir}>
              Create
            </button>
          </div>
        )}

        {/* Files List Table */}
        <div style={{ flex: 1, overflowY: "auto", padding: "8px 16px" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.88rem" }}>
            <thead>
              <tr style={{ borderBottom: "1px solid var(--border-color)", textAlign: "left", opacity: 0.7 }}>
                <th style={{ padding: 8 }}>Name</th>
                <th style={{ padding: 8 }}>Size</th>
                <th style={{ padding: 8 }}>Modified</th>
                <th style={{ padding: 8, textAlign: "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {files.length === 0 ? (
                <tr>
                  <td colSpan={4} style={{ textAlign: "center", padding: 24, opacity: 0.6 }}>
                    {loading ? "Loading files..." : "Empty directory."}
                  </td>
                </tr>
              ) : (
                files.map((file) => (
                  <tr key={file.path} style={{ borderBottom: "1px solid rgba(255,255,255,0.05)" }}>
                    <td
                      style={{ padding: 8, cursor: file.isDir ? "pointer" : "default", fontWeight: file.isDir ? 600 : 400 }}
                      onClick={() => handleOpenItem(file)}
                    >
                      {file.isDir ? "📁" : "📄"} {file.name}
                    </td>
                    <td style={{ padding: 8, opacity: 0.7 }}>{formatSize(file.size)}</td>
                    <td style={{ padding: 8, opacity: 0.6, fontSize: "0.8rem" }}>{file.modified}</td>
                    <td style={{ padding: 8, textAlign: "right" }}>
                      {!file.isDir && (
                        <button
                          className="button button-secondary button-sm"
                          onClick={() => handlePull(file)}
                          style={{ marginRight: 6, fontSize: "0.75rem", padding: "2px 6px" }}
                        >
                          Download
                        </button>
                      )}
                      <button
                        className="button button-danger button-sm"
                        onClick={() => handleDelete(file)}
                        style={{ fontSize: "0.75rem", padding: "2px 6px" }}
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
