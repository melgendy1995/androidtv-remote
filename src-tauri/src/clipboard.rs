use crate::adb::AdbClient;
use crate::error::Result;

pub async fn get_clipboard(adb: &AdbClient, serial: &str) -> Result<String> {
    let out = adb.shell(serial, "cmd clipboard get").await.unwrap_or_default();
    if !out.trim().is_empty() && !out.contains("Error") {
        Ok(out.trim().to_string())
    } else {
        let dump = adb.shell(serial, "dumpsys clipboard").await.unwrap_or_default();
        for line in dump.lines() {
            if line.contains("text=") {
                if let Some((_, text)) = line.split_once("text=") {
                    return Ok(text.trim_matches(|c| c == '\'' || c == '"' || c == ' ').to_string());
                }
            }
        }
        Ok(String::new())
    }
}

pub async fn set_clipboard(adb: &AdbClient, serial: &str, text: &str) -> Result<()> {
    let escaped = text.replace('"', "\\\"").replace('$', "\\$");
    let cmd = format!("cmd clipboard set \"{escaped}\"");
    let out = adb.shell(serial, &cmd).await;
    if out.is_err() || out.as_ref().map(|s| s.contains("Error")).unwrap_or(false) {
        let input_cmd = crate::keys::escape_input_text(text);
        let _ = adb.shell(serial, &format!("input text {input_cmd}")).await;
    }
    Ok(())
}
