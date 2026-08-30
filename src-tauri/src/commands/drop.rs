use crate::state::AppState;
use base64::Engine;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::State;

const MAX_STAGED_DROP_BYTES: usize = 20 * 1024 * 1024;
const MAX_STAGED_DROP_BASE64_BYTES: usize = (MAX_STAGED_DROP_BYTES / 3) * 4 + 8;
const DROP_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

fn drop_directory() -> PathBuf {
    std::env::temp_dir().join("athena-drop-files")
}

fn extension_for(file_name: &str, mime_type: &str) -> Option<&'static str> {
    let lower_name = file_name.to_ascii_lowercase();
    for (suffix, extension) in [
        (".png", "png"),
        (".jpg", "jpg"),
        (".jpeg", "jpg"),
        (".gif", "gif"),
        (".webp", "webp"),
        (".heic", "heic"),
        (".tif", "tif"),
        (".tiff", "tiff"),
    ] {
        if lower_name.ends_with(suffix) {
            return Some(extension);
        }
    }
    match mime_type.to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/heic" => Some("heic"),
        "image/tiff" => Some("tiff"),
        _ => None,
    }
}

fn clean_old_drop_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if SystemTime::now()
            .duration_since(modified)
            .is_ok_and(|age| age > DROP_RETENTION)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn stage_drop_file_sync(
    file_name: String,
    mime_type: String,
    base64_data: String,
) -> Result<String, String> {
    if base64_data.len() > MAX_STAGED_DROP_BASE64_BYTES {
        return Err(format!(
            "dropped image is too large (maximum {} bytes)",
            MAX_STAGED_DROP_BYTES
        ));
    }
    let mime_type = mime_type.to_ascii_lowercase();
    if !mime_type.starts_with("image/") && extension_for(&file_name, &mime_type).is_none() {
        return Err("only image drops are supported without a native path".to_string());
    }
    let extension = extension_for(&file_name, &mime_type)
        .ok_or_else(|| "unsupported dropped image type".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.as_bytes())
        .map_err(|_| "dropped image data is not valid base64".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_STAGED_DROP_BYTES {
        return Err(format!(
            "dropped image is too large or empty (maximum {} bytes)",
            MAX_STAGED_DROP_BYTES
        ));
    }

    let directory = drop_directory();
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    clean_old_drop_files(&directory);

    let path = directory.join(format!("drop-{}.{}", uuid::Uuid::new_v4(), extension));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn pty_stage_drop_file(
    state: State<'_, AppState>,
    file_name: String,
    mime_type: String,
    base64_data: String,
) -> Result<String, String> {
    if !state.rate_limiter.check("pty_stage_drop_file") {
        return Err("rate limit exceeded; please wait a moment".to_string());
    }
    tokio::task::spawn_blocking(move || stage_drop_file_sync(file_name, mime_type, base64_data))
        .await
        .map_err(|error| format!("drop staging task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::extension_for;

    #[test]
    fn extension_uses_safe_image_allowlist() {
        assert_eq!(extension_for("Screenshot.PNG", ""), Some("png"));
        assert_eq!(extension_for("capture", "image/jpeg"), Some("jpg"));
        assert_eq!(extension_for("notes.txt", "text/plain"), None);
    }
}
