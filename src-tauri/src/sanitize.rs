use crate::error::{AppError, Result};

/// Escape a value for safe interpolation into a double-quoted region of an
/// adb shell command line (`shell cmd "...{value}..."`).
///
/// Inside double quotes the device shell still expands `$var`, backtick
/// substitution and `\"`, so all four characters are backslash-escaped.
pub fn escape_shell_arg(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '\\' | '"' | '$' | '`' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Validate a remote path before it reaches the device shell. Rejects any
/// character that could break out of quoting or act as shell syntax.
pub fn validate_remote_path(path: &str) -> Result<&str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::from("path is empty"));
    }
    let bad: Vec<char> = trimmed
        .chars()
        .filter(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '_' | '-' | ' ' | '(' | ')' | '[' | ']' | ',' | '\''))
        .collect();
    if !bad.is_empty() {
        return Err(AppError::from(format!(
            "path contains unsupported characters: {}",
            bad.iter().collect::<String>()
        )));
    }
    Ok(trimmed)
}

/// Validate an Android package name (`^[A-Za-z0-9._]+$`) before it reaches
/// the device shell or `adb uninstall`.
pub fn validate_package(package_name: &str) -> Result<&str> {
    let trimmed = package_name.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        || trimmed.starts_with('.')
    {
        return Err(AppError::from(format!(
            "invalid package name: {package_name}"
        )));
    }
    Ok(trimmed)
}

/// Validate a hostname / IPv4 literal used in `settings put global http_proxy`.
pub fn validate_host(host: &str) -> Result<&str> {
    let trimmed = host.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
    {
        return Err(AppError::from(format!("invalid host: {host}")));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_and_substitutions() {
        assert_eq!(escape_shell_arg("a\"b"), "a\\\"b");
        assert_eq!(escape_shell_arg("a$b"), "a\\$b");
        assert_eq!(escape_shell_arg("a`rm`b"), "a\\`rm\\`b");
        assert_eq!(escape_shell_arg("back\\slash"), "back\\\\slash");
        assert_eq!(escape_shell_arg("plain path/file.txt"), "plain path/file.txt");
    }

    #[test]
    fn rejects_injection_paths() {
        assert!(validate_remote_path("/sdcard/x\"; reboot; \"").is_err());
        assert!(validate_remote_path("/sdcard/`id`").is_err());
        assert!(validate_remote_path("/sdcard/a$b").is_err());
        assert!(validate_remote_path("/sdcard/a;b").is_err());
        assert!(validate_remote_path("").is_err());
        assert!(validate_remote_path("/sdcard/Movies/Captures.v2").is_ok());
        assert!(validate_remote_path("/sdcard/My Videos (2026)").is_ok());
    }

    #[test]
    fn rejects_injection_packages() {
        assert!(validate_package("com.intigral.jawwytv").is_ok());
        assert!(validate_package("com.foo; rm -rf /").is_err());
        assert!(validate_package(".hidden").is_err());
        assert!(validate_package("").is_err());
    }

    #[test]
    fn rejects_bad_hosts() {
        assert!(validate_host("192.168.1.10").is_ok());
        assert!(validate_host("proxy.corp.local").is_ok());
        assert!(validate_host("1.2.3.4; reboot").is_err());
        assert!(validate_host("`reboot`").is_err());
        assert!(validate_host("").is_err());
    }
}
