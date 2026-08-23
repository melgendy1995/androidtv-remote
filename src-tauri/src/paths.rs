use std::path::{Path, PathBuf};
use tauri::Manager;

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".androidtv-remote")
}

pub fn devices_path() -> PathBuf {
    config_dir().join("devices.json")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn default_capture_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AndroidTV Captures")
}

/// Create `path` as a directory (including parents). Callers pass the exact
/// directory they want — no extension sniffing, so dotted directory names
/// like "captures.v2" work.
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

/// Create the parent directory of a FILE path.
pub fn ensure_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

pub fn resource_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}
