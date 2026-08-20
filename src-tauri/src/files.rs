use serde::Serialize;
use std::path::Path;

use crate::adb::AdbClient;
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

pub async fn list_files(adb: &AdbClient, serial: &str, target_path: &str) -> Result<Vec<RemoteFile>> {
    let path = if target_path.trim().is_empty() {
        "/sdcard"
    } else {
        target_path.trim()
    };

    let cmd = format!("ls -la \"{path}\"");
    let output = adb.shell(serial, &cmd).await?;

    let mut files = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }

        let permissions = parts[0];
        let is_dir = permissions.starts_with('d') || permissions.starts_with('l');

        let mut name_idx = 6;
        let mut size: u64 = 0;
        let mut modified = String::new();

        if parts.len() >= 8 {
            if let Ok(parsed_size) = parts[4].parse::<u64>() {
                size = parsed_size;
                modified = format!("{} {}", parts[5], parts[6]);
                name_idx = 7;
            } else if let Ok(parsed_size) = parts[3].parse::<u64>() {
                size = parsed_size;
                modified = format!("{} {}", parts[4], parts[5]);
                name_idx = 6;
            }
        }

        let name = parts[name_idx..].join(" ");
        if name == "." || name == ".." {
            continue;
        }

        let full_path = if path.ends_with('/') {
            format!("{path}{name}")
        } else {
            format!("{path}/{name}")
        };

        files.push(RemoteFile {
            name,
            path: full_path,
            is_dir,
            size,
            modified,
        });
    }

    files.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    Ok(files)
}

pub async fn pull_file(
    adb: &AdbClient,
    serial: &str,
    remote_path: &str,
    local_path: &str,
) -> Result<()> {
    let out = adb
        .run_serial(Some(serial), &["pull", remote_path, local_path])
        .await?;
    if out.contains("error") || out.contains("failed") {
        Err(AppError::from(format!("Pull failed: {out}")))
    } else {
        Ok(())
    }
}

pub async fn push_file(
    adb: &AdbClient,
    serial: &str,
    local_path: &str,
    remote_path: &str,
) -> Result<()> {
    adb.push(serial, Path::new(local_path), remote_path).await
}

pub async fn delete_file(adb: &AdbClient, serial: &str, remote_path: &str) -> Result<()> {
    let cmd = format!("rm -rf \"{remote_path}\"");
    adb.shell(serial, &cmd).await?;
    Ok(())
}

pub async fn mkdir_remote(adb: &AdbClient, serial: &str, remote_path: &str) -> Result<()> {
    let cmd = format!("mkdir -p \"{remote_path}\"");
    adb.shell(serial, &cmd).await?;
    Ok(())
}
