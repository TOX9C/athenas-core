use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const MAX_LOG_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_LOG_BYTES: usize = 8 * 1024 * 1024;
const MAX_FRONTEND_BYTES: usize = 512 * 1024;

/// Export a support-safe diagnostic bundle to the user's Downloads directory.
///
/// The bundle intentionally contains only bounded, redacted runtime logs and
/// frontend performance/error diagnostics. It does not read the workspace
/// store, chat sessions, API key store, or terminal output history.
#[tauri::command]
pub fn diagnostics_export(
    frontend_logs: String,
    frontend_metrics: String,
) -> Result<String, String> {
    let home = dirs::home_dir();
    let log_dir = platform_log_dir();
    let backend_logs = collect_backend_logs(log_dir.as_deref(), home.as_deref());
    let frontend_logs = redact_and_cap(&frontend_logs, home.as_deref(), MAX_FRONTEND_BYTES);
    let frontend_metrics = redact_and_cap(&frontend_metrics, home.as_deref(), MAX_FRONTEND_BYTES);

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock error: {e}"))?
        .as_millis();
    let output_dir = dirs::download_dir()
        .or_else(|| dirs::data_dir().map(|dir| dir.join("athena-core").join("diagnostics")))
        .ok_or_else(|| "could not locate a writable diagnostics directory".to_string())?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("could not create diagnostics directory: {e}"))?;

    let bundle = json!({
        "format": "athena-diagnostics-v1",
        "generated_at_unix_ms": timestamp_ms,
        "app_version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "backend_logs": backend_logs,
        "frontend_errors_and_warnings": frontend_logs,
        "frontend_metrics": frontend_metrics,
    });
    let output_path = output_dir.join(format!(
        "athena-diagnostics-{timestamp_ms}-{}.json",
        Uuid::new_v4().simple()
    ));
    let serialized = serde_json::to_vec_pretty(&bundle)
        .map_err(|e| format!("could not serialize diagnostics: {e}"))?;
    std::fs::write(&output_path, serialized)
        .map_err(|e| format!("could not write diagnostics bundle: {e}"))?;

    log::info!(
        "[diagnostics] exported redacted bundle bytes={} backend_log_dir_present={}",
        output_path.metadata().map(|m| m.len()).unwrap_or(0),
        log_dir.is_some_and(|path| path.is_dir())
    );
    Ok(output_path.to_string_lossy().into_owned())
}

fn platform_log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library/Logs/com.athena.core"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|dir| dir.join("com.athena.core").join("logs"))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::data_local_dir().map(|dir| dir.join("com.athena.core").join("logs"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        dirs::data_local_dir().map(|dir| dir.join("com.athena.core").join("logs"))
    }
}

fn collect_backend_logs(log_dir: Option<&Path>, home: Option<&Path>) -> Vec<serde_json::Value> {
    let Some(log_dir) = log_dir else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Vec::new();
    };
    // Prefer the newest rotated files and take their tails. This makes the
    // bundle useful for a "what happened just before the freeze?" report
    // instead of spending the cap on the oldest retained archive.
    let mut paths: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((path, modified))
        })
        .collect();
    paths.sort_by(|(left_path, left_time), (right_path, right_time)| {
        right_time
            .cmp(left_time)
            .then_with(|| right_path.cmp(left_path))
    });

    let mut total = 0usize;
    paths
        .into_iter()
        .filter_map(|(path, _)| {
            if total >= MAX_TOTAL_LOG_BYTES {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            let remaining = MAX_TOTAL_LOG_BYTES - total;
            let take = bytes.len().min(MAX_LOG_FILE_BYTES).min(remaining);
            total += take;
            let start = bytes.len().saturating_sub(take);
            let text = String::from_utf8_lossy(&bytes[start..]);
            let file_name = path.file_name()?.to_string_lossy().into_owned();
            Some(json!({
                "file": file_name,
                "truncated": take < bytes.len(),
                "content": redact_and_cap(&text, home, take),
            }))
        })
        .collect()
}

fn redact_and_cap(text: &str, home: Option<&Path>, max_bytes: usize) -> String {
    let normalized = home
        .and_then(|path| path.to_str())
        .map(|path| text.replace(path, "<HOME>"))
        .unwrap_or_else(|| text.to_string());
    let mut output = String::new();
    for line in normalized.lines() {
        let lower = line.to_ascii_lowercase();
        let sensitive = [
            "authorization",
            "bearer ",
            "api_key",
            "api-key",
            "apikey",
            "password",
            "secret",
            "credential",
            "access_token",
            "refresh_token",
            "\"token\"",
            "token:",
            "token=",
            "\"key\"",
            "key:",
            "key=",
            "sk-",
            "ghp_",
            "xoxb-",
        ]
        .iter()
        .any(|marker| lower.contains(marker));
        let safe_line = if sensitive {
            "<REDACTED SENSITIVE LOG LINE>"
        } else {
            line
        };
        if output.len() + safe_line.len() + 1 > max_bytes {
            break;
        }
        output.push_str(safe_line);
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::redact_and_cap;
    use std::path::Path;

    #[test]
    fn redacts_home_paths_and_sensitive_lines() {
        let input = "path=/Users/alice/project\nauthorization: Bearer secret\nnormal warning\n";
        let output = redact_and_cap(input, Some(Path::new("/Users/alice")), 4096);
        assert!(output.contains("<HOME>/project"));
        assert!(output.contains("<REDACTED SENSITIVE LOG LINE>"));
        assert!(output.contains("normal warning"));
        assert!(!output.contains("Bearer secret"));
    }

    #[test]
    fn caps_exported_text() {
        let output = redact_and_cap("1234567890\n", None, 5);
        assert!(output.len() <= 5);
    }

    #[test]
    fn redacts_json_key_and_token_formats() {
        let input = r#"request={\"apiKey\":\"sk-live-secret\",\"token\":\"private\"}"#;
        let output = redact_and_cap(input, None, 4096);
        assert_eq!(output.trim(), "<REDACTED SENSITIVE LOG LINE>");
        assert!(!output.contains("sk-live-secret"));
        assert!(!output.contains("private"));
    }
}
