use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::paths::{ensure_dir, settings_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub adb_path: String,
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,
    #[serde(default)]
    pub capture_dir: String,
    #[serde(default = "default_max_size")]
    pub max_size: u32,
    #[serde(default = "default_bit_rate")]
    pub bit_rate: u32,
    #[serde(default = "default_max_fps")]
    pub max_fps: u32,
    #[serde(default)]
    pub audio_enabled: bool,
    #[serde(default = "default_device_proxy_mode")]
    pub device_proxy_mode: String,
    #[serde(default)]
    pub charles_host: String,
    #[serde(default = "default_charles_port")]
    pub charles_port: u16,
}

fn default_proxy_port() -> u16 {
    8899
}
fn default_max_size() -> u32 {
    1920
}
fn default_bit_rate() -> u32 {
    8000000
}
fn default_max_fps() -> u32 {
    60
}
fn default_device_proxy_mode() -> String {
    "builtin".into()
}
fn default_charles_port() -> u16 {
    8888
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            adb_path: String::new(),
            proxy_port: 8888,
            capture_dir: String::new(),
            max_size: 1920,
            bit_rate: 8000000,
            max_fps: 60,
            audio_enabled: false,
            device_proxy_mode: default_device_proxy_mode(),
            charles_host: String::new(),
            charles_port: default_charles_port(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> crate::error::Result<()> {
        let path = settings_path();
        ensure_dir(&path)?;
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn capture_dir_path(&self) -> PathBuf {
        if self.capture_dir.trim().is_empty() {
            crate::paths::default_capture_dir()
        } else {
            PathBuf::from(&self.capture_dir)
        }
    }
}
