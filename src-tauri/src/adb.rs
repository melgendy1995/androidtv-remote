use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::error::{AppError, Result};
use crate::settings::Settings;

#[derive(Clone)]
pub struct AdbClient {
    binary: PathBuf,
}

impl AdbClient {
    pub fn new(settings: &Settings, resource_dir: &Path) -> Self {
        let binary = resolve_adb(&settings.adb_path, resource_dir);
        Self { binary }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    pub async fn version(&self) -> Result<String> {
        let out = self.run(&["version"]).await?;
        Ok(out.lines().next().unwrap_or("adb").to_string())
    }

    pub async fn enable_tcpip(&self, serial: &str, port: u16) -> Result<String> {
        self.run_serial(Some(serial), &["tcpip", &port.to_string()]).await
    }

    pub async fn run(&self, args: &[&str]) -> Result<String> {
        self.run_serial(None, args).await
    }

    pub async fn run_serial(&self, serial: Option<&str>, args: &[&str]) -> Result<String> {
        self.run_serial_timeout(serial, args, Duration::from_secs(20))
            .await
    }

    pub async fn run_serial_timeout(
        &self,
        serial: Option<&str>,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String> {
        let mut cmd = adb_command(&self.binary);
        if let Some(serial) = serial {
            cmd.arg("-s").arg(serial);
        }
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| AppError::from("adb timed out"))?
            .map_err(|e| AppError::from(format!("failed to spawn adb: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            let msg = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            return Err(AppError::from(if msg.is_empty() {
                format!("adb {} failed", args.join(" "))
            } else {
                msg
            }));
        }
        Ok(stdout)
    }

    pub async fn shell(&self, serial: &str, command: &str) -> Result<String> {
        self.run_serial(Some(serial), &["shell", command]).await
    }

    pub async fn get_state(&self, serial: &str) -> Result<String> {
        Ok(self
            .run_serial(Some(serial), &["get-state"])
            .await?
            .trim()
            .to_string())
    }

    pub async fn connect_host(&self, host: &str) -> Result<String> {
        let serial = normalize_host(host);
        self.run(&["connect", &serial]).await?;
        Ok(serial)
    }

    pub async fn pair(&self, host: &str, code: &str) -> Result<String> {
        self.run(&["pair", host, code]).await
    }

    pub async fn disconnect(&self, serial: &str) -> Result<()> {
        let _ = self.run(&["disconnect", serial]).await;
        Ok(())
    }

    pub async fn prop(&self, serial: &str, key: &str) -> Result<String> {
        Ok(self
            .shell(serial, &format!("getprop {key}"))
            .await?
            .trim()
            .to_string())
    }

    pub async fn devices_raw(&self) -> Result<String> {
        self.run(&["devices", "-l"]).await
    }

    pub async fn push(&self, serial: &str, src: &Path, dest: &str) -> Result<()> {
        let src_s = strip_verbatim_prefix(src).to_string_lossy().to_string();
        self.run_serial_timeout(
            Some(serial),
            &["push", &src_s, dest],
            Duration::from_secs(120),
        )
        .await?;
        Ok(())
    }

    pub async fn reverse(&self, serial: &str, remote: &str, local: &str) -> Result<()> {
        let _ = self
            .run_serial(Some(serial), &["reverse", "--remove", remote])
            .await;
        self.run_serial(Some(serial), &["reverse", remote, local])
            .await?;
        Ok(())
    }

    pub async fn forward(&self, serial: &str, local: &str, remote: &str) -> Result<()> {
        self.run_serial(Some(serial), &["forward", local, remote])
            .await?;
        Ok(())
    }

    pub async fn exec_out(&self, serial: &str, command: &str) -> Result<Vec<u8>> {
        let mut cmd = adb_command(&self.binary);
        cmd.arg("-s")
            .arg(serial)
            .arg("exec-out")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(20), cmd.output())
            .await
            .map_err(|_| AppError::from("adb exec-out timed out"))??;
        if !output.status.success() {
            return Err(AppError::from(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(output.stdout)
    }

    pub fn spawn_logcat(&self, serial: &str) -> Result<tokio::process::Child> {
        let child = adb_command(&self.binary)
            .arg("-s")
            .arg(serial)
            .args(["logcat", "-v", "threadtime"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AppError::from(format!("logcat spawn failed: {e}")))?;
        Ok(child)
    }

    pub fn spawn_shell(&self, serial: &str, command: &str) -> Result<tokio::process::Child> {
        let child = adb_command(&self.binary)
            .arg("-s")
            .arg(serial)
            .arg("shell")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AppError::from(format!("adb shell spawn failed: {e}")))?;
        Ok(child)
    }
}

pub fn normalize_host(host: &str) -> String {
    let host = host.trim();
    if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:5555")
    }
}

pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

fn adb_command(binary: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(binary);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn resolve_adb(override_path: &str, resource_dir: &Path) -> PathBuf {
    let trimmed = override_path.trim();
    if !trimmed.is_empty() {
        return PathBuf::from(trimmed);
    }
    let names: &[&str] = if cfg!(windows) {
        &["adb.exe", "adb"]
    } else {
        &["adb"]
    };
    let bases = [resource_dir.join("resources"), resource_dir.to_path_buf()];
    for base in bases {
        for name in names {
            let candidate = base.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    which::which("adb")
        .or_else(|_| which::which("adb.exe"))
        .unwrap_or_else(|_| PathBuf::from(if cfg!(windows) { "adb.exe" } else { "adb" }))
}

#[derive(Debug, Clone)]
pub struct ListedDevice {
    pub serial: String,
    pub state: String,
    pub model: String,
}

pub fn parse_devices(raw: &str) -> Vec<ListedDevice> {
    raw.lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let mut model = String::new();
            for part in parts {
                if let Some(value) = part.strip_prefix("model:") {
                    model = value.replace('_', " ");
                }
            }
            Some(ListedDevice {
                serial,
                state,
                model,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_windows_verbatim_prefix() {
        let path = PathBuf::from(r"\\?\C:\Program Files\app\scrcpy-server");
        assert_eq!(
            strip_verbatim_prefix(&path),
            PathBuf::from(r"C:\Program Files\app\scrcpy-server")
        );
    }
}
