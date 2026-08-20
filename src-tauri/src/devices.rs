use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adb::{parse_devices, AdbClient, ListedDevice};
use crate::error::Result;
use crate::paths::{devices_path, ensure_dir};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDevice {
    pub id: String,
    pub name: String,
    pub serial: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub last_connected_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub serial: String,
    pub name: String,
    pub model: String,
    pub android_version: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
    pub unauthorized: bool,
    pub adb_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adb_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adb_error: Option<String>,
}

#[derive(Default)]
pub struct DeviceRegistry {
    saved: HashMap<String, SavedDevice>,
}

impl DeviceRegistry {
    pub fn load() -> Self {
        let path = devices_path();
        let saved = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<SavedDevice>>(&raw).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|d| (d.serial.clone(), d))
            .collect();
        Self { saved }
    }

    fn persist(&self) -> Result<()> {
        let path = devices_path();
        ensure_dir(&path)?;
        let list: Vec<&SavedDevice> = self.saved.values().collect();
        std::fs::write(&path, serde_json::to_vec_pretty(&list)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn get(&self, serial: &str) -> Option<&SavedDevice> {
        self.saved.get(serial)
    }

    pub fn forget(&mut self, serial: &str) -> Result<()> {
        self.saved.remove(serial);
        self.persist()
    }

    pub fn remember(&mut self, device: DeviceInfo) -> Result<SavedDevice> {
        let now = now_ms();
        let saved = SavedDevice {
            id: device.serial.clone(),
            name: device.name.clone(),
            serial: device.serial.clone(),
            host: device.host.clone(),
            last_connected_at: Some(now),
        };
        self.saved.insert(saved.serial.clone(), saved.clone());
        self.persist()?;
        Ok(saved)
    }

    pub fn merge_listed(&self, listed: Vec<ListedDevice>) -> Vec<DeviceInfo> {
        let mut out: Vec<DeviceInfo> = listed
            .into_iter()
            .map(|d| {
                let saved = self.saved.get(&d.serial);
                DeviceInfo {
                    name: saved
                        .map(|s| s.name.clone())
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| {
                            if d.model.is_empty() {
                                d.serial.clone()
                            } else {
                                d.model.clone()
                            }
                        }),
                    serial: d.serial.clone(),
                    model: d.model,
                    android_version: String::new(),
                    state: d.state,
                    host: saved.and_then(|s| s.host.clone()).or_else(|| {
                        if d.serial.contains(':') {
                            Some(d.serial.clone())
                        } else {
                            None
                        }
                    }),
                    saved: saved.is_some(),
                    last_connected_at: saved.and_then(|s| s.last_connected_at),
                }
            })
            .collect();

        for saved in self.saved.values() {
            if !out.iter().any(|d| d.serial == saved.serial) {
                out.push(DeviceInfo {
                    serial: saved.serial.clone(),
                    name: saved.name.clone(),
                    model: String::new(),
                    android_version: String::new(),
                    state: "offline".into(),
                    host: saved.host.clone(),
                    saved: true,
                    last_connected_at: saved.last_connected_at,
                });
            }
        }
        out.sort_by(|a, b| b.last_connected_at.cmp(&a.last_connected_at));
        out
    }
}

pub async fn enrich(adb: &AdbClient, device: &mut DeviceInfo) {
    if device.state != "device" {
        return;
    }
    if let Ok(model) = adb.prop(&device.serial, "ro.product.model").await {
        if !model.is_empty() {
            device.model = model.clone();
            if device.name == device.serial {
                device.name = model;
            }
        }
    }
    if let Ok(ver) = adb.prop(&device.serial, "ro.build.version.release").await {
        device.android_version = ver;
    }
}

pub async fn list_merged(adb: &AdbClient, registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>> {
    let raw = adb.devices_raw().await?;
    let listed = parse_devices(&raw);
    let mut devices = registry.merge_listed(listed);
    for device in devices.iter_mut().filter(|d| d.state == "device") {
        enrich(adb, device).await;
    }
    Ok(devices)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
