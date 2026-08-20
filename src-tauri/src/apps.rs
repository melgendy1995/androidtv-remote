use serde::Serialize;

use crate::adb::AdbClient;
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub package_name: String,
    pub label: String,
    pub is_system: bool,
}

pub async fn list_apps(adb: &AdbClient, serial: &str) -> Result<Vec<AppInfo>> {
    let raw_user = adb.shell(serial, "pm list packages -3").await.unwrap_or_default();
    let raw_sys = adb.shell(serial, "pm list packages -s").await.unwrap_or_default();

    let mut apps = Vec::new();

    for line in raw_user.lines() {
        if let Some(pkg) = parse_package_line(line) {
            let label = friendly_label(&pkg);
            apps.push(AppInfo {
                package_name: pkg,
                label,
                is_system: false,
            });
        }
    }

    for line in raw_sys.lines() {
        if let Some(pkg) = parse_package_line(line) {
            let label = friendly_label(&pkg);
            apps.push(AppInfo {
                package_name: pkg,
                label,
                is_system: true,
            });
        }
    }

    apps.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    Ok(apps)
}

fn parse_package_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("package:") {
        let pkg = if let Some((_, right)) = rest.split_once('=') {
            right
        } else {
            rest
        };
        let pkg = pkg.trim();
        if !pkg.is_empty() {
            return Some(pkg.to_string());
        }
    }
    None
}

fn friendly_label(pkg: &str) -> String {
    let parts: Vec<&str> = pkg.split('.').collect();
    let raw = if parts.len() > 1 {
        let last = parts.last().unwrap();
        if *last == "android" || *last == "app" || *last == "tv" {
            if parts.len() > 2 {
                parts[parts.len() - 2]
            } else {
                last
            }
        } else {
            last
        }
    } else {
        pkg
    };

    let mut chars = raw.chars();
    match chars.next() {
        None => pkg.to_string(),
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub async fn launch_app(adb: &AdbClient, serial: &str, package_name: &str) -> Result<()> {
    let cmd = format!("monkey -p {package_name} -c android.intent.category.LAUNCHER 1");
    let out = adb.shell(serial, &cmd).await?;
    if out.contains("No activities found to run") {
        let cmd_tv = format!("monkey -p {package_name} -c android.intent.category.LEANBACK_LAUNCHER 1");
        let _ = adb.shell(serial, &cmd_tv).await;
    }
    Ok(())
}

pub async fn force_stop_app(adb: &AdbClient, serial: &str, package_name: &str) -> Result<()> {
    let cmd = format!("am force-stop {package_name}");
    adb.shell(serial, &cmd).await?;
    Ok(())
}

pub async fn install_apk(adb: &AdbClient, serial: &str, file_path: &str) -> Result<String> {
    let out = adb
        .run_serial_timeout(
            Some(serial),
            &["install", "-r", file_path],
            std::time::Duration::from_secs(300),
        )
        .await?;
    if out.contains("Success") {
        Ok("APK installed successfully!".to_string())
    } else {
        Err(AppError::from(format!("Failed to install APK: {out}")))
    }
}

pub async fn uninstall_app(adb: &AdbClient, serial: &str, package_name: &str) -> Result<()> {
    let out = adb.run_serial(Some(serial), &["uninstall", package_name]).await?;
    if out.contains("Success") {
        Ok(())
    } else {
        Err(AppError::from(format!("Uninstall failed: {out}")))
    }
}
