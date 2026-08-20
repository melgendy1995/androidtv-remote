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

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = if path.extension().is_some() {
        path.parent()
    } else {
        Some(path)
    } {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn resource_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}
