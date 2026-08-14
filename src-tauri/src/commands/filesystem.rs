use super::{caps, validate_path, validate_path_exists, CommandError};
use crate::state::AppState;
use base64::Engine;
use std::io::Write;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// Atomically replace a file without following a planted temporary-file symlink.
///
/// The destination has already been validated by the command boundary. This
/// helper closes the separate temporary-file race by creating the temporary
/// entry with `create_new` and writing through the opened handle, rather than
/// using `fs::write` on a predictable path.
fn write_file_atomic(path: &std::path::Path, content: &[u8]) -> Result<(), std::io::Error> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let temp_path = parent.join(format!(".tmp-write-{}", uuid::Uuid::new_v4()));

    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(content)?;
        file.flush()?;
        std::fs::rename(&temp_path, path)
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::write_file_atomic;
    use std::fs;

    #[test]
    fn atomic_write_replaces_target_without_leaving_temp_files() {
        let root = std::env::temp_dir().join(format!("athena-fs-command-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("target.txt");
        fs::write(&target, "old").unwrap();

        write_file_atomic(&target, b"new").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_replaces_destination_symlink_instead_of_following_it() {
        let root = std::env::temp_dir().join(format!("athena-fs-link-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside.txt");
        let target = root.join("target.txt");
        fs::write(&outside, "must remain").unwrap();
        std::os::unix::fs::symlink(&outside, &target).unwrap();

        write_file_atomic(&target, b"inside").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "inside");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "must remain");
        assert!(!fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_dir_all(root).unwrap();
    }
}

/// Read the contents of a file as UTF-8 text.
#[tauri::command]
pub async fn fs_read_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, CommandError> {
    if !state.rate_limiter.check("fs_read_file") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    let validated_clone = validated.clone();
    tokio::task::spawn_blocking(move || {
        // Check file size before reading to prevent memory exhaustion.
        let metadata = std::fs::metadata(&validated_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        if metadata.len() > caps::MAX_FS_READ_BYTES as u64 {
            return Err(CommandError::InvalidInput(format!(
                "file too large: {} bytes (max {})",
                metadata.len(),
                caps::MAX_FS_READ_BYTES
            )));
        }
        std::fs::read_to_string(&validated_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
}

#[derive(serde::Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// List the contents of a directory, sorted with directories first.
#[tauri::command]
pub async fn fs_list_dir(state: State<'_, AppState>, path: String) -> Result<String, CommandError> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    tokio::task::spawn_blocking(move || {
        let mut entries: Vec<DirEntry> = Vec::new();
        let read_dir =
            std::fs::read_dir(&validated).map_err(|e| CommandError::Internal(e.to_string()))?;
        for entry_result in read_dir {
            let entry = entry_result.map_err(|e| CommandError::Internal(e.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path().to_string_lossy().to_string();
            let is_dir = file_type.is_dir();
            entries.push(DirEntry { name, path, is_dir });
        }
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        serde_json::to_string(&entries).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
}

/// Write content to a file, creating it if it doesn't exist.
#[tauri::command]
pub async fn fs_write_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
) -> Result<(), CommandError> {
    if !state.rate_limiter.check("fs_write_file") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    if content.len() > caps::MAX_FS_WRITE_BYTES {
        return Err(CommandError::InvalidInput(format!(
            "content too large: {} > {}",
            content.len(),
            caps::MAX_FS_WRITE_BYTES
        )));
    }
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path(&state.store, path_ref)?;
    let validated_clone = validated.clone();
    let content_clone = content.clone();
    tokio::task::spawn_blocking(move || {
        write_file_atomic(&validated_clone, content_clone.as_bytes())
            .map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Write task failed: {e}")))?
}

/// Check whether a path exists and is within the allowed directory.
///
/// Synchronous (not `async`) because Tauri forbids async commands that return
/// a bare non-`Result` type. Sync commands run on the runtime's blocking
/// thread, so the canonicalize inside `validate_path_exists` won't stall the
/// async executor.
#[tauri::command]
pub fn fs_exists(state: State<'_, AppState>, path: String) -> bool {
    let path_ref = std::path::Path::new(&path);
    validate_path_exists(&state.store, path_ref).is_ok()
}

/// Read a file and return its contents as a base64-encoded string.
#[tauri::command]
pub async fn fs_read_file_as_base64(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, CommandError> {
    if !state.rate_limiter.check("fs_read_file_as_base64") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    let validated_clone = validated.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        // Check file size before reading to prevent memory exhaustion.
        let metadata = std::fs::metadata(&validated_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        if metadata.len() > caps::MAX_FS_READ_BYTES as u64 {
            return Err(CommandError::InvalidInput(format!(
                "file too large: {} bytes (max {})",
                metadata.len(),
                caps::MAX_FS_READ_BYTES
            )));
        }
        std::fs::read(&validated_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))??;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Show a native file/folder open dialog and return the selected path(s).
#[tauri::command]
pub async fn fs_show_open_dialog(
    app_handle: AppHandle,
    title: Option<String>,
    multiple: Option<bool>,
    directory: Option<bool>,
) -> Result<String, String> {
    let is_directory = directory.unwrap_or(false);
    let is_multiple = multiple.unwrap_or(false);

    let mut dialog = app_handle.dialog().file();
    if let Some(t) = &title {
        dialog = dialog.set_title(t);
    }

    let result = tokio::task::spawn_blocking(move || match (is_directory, is_multiple) {
        (true, false) => dialog
            .blocking_pick_folder()
            .map(|fp| fp.to_string())
            .unwrap_or_default(),
        (true, true) => dialog
            .blocking_pick_folders()
            .map(|list| {
                list.iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        (false, false) => dialog
            .blocking_pick_file()
            .map(|fp| fp.to_string())
            .unwrap_or_default(),
        (false, true) => dialog
            .blocking_pick_files()
            .map(|list| {
                list.iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
    })
    .await
    .map_err(|e| format!("Dialog task failed: {e}"))?;

    Ok(result)
}

/// Show a native file dialog filtered to image types (png, jpg, jpeg, gif, svg, webp).
#[tauri::command]
pub async fn fs_show_image_dialog(app_handle: AppHandle) -> Result<String, String> {
    let dialog = app_handle
        .dialog()
        .file()
        .set_title("Select Image")
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "svg", "webp"]);

    let result = tokio::task::spawn_blocking(move || {
        dialog
            .blocking_pick_file()
            .map(|fp| fp.to_string())
            .unwrap_or_default()
    })
    .await
    .map_err(|e| format!("Dialog task failed: {e}"))?;

    Ok(result)
}

/// Search files in a directory using ripgrep with the given pattern.
#[tauri::command]
pub async fn fs_search_files(
    state: State<'_, AppState>,
    pattern: String,
    path: String,
) -> Result<String, String> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref).map_err(|e| e.to_string())?;
    let options = athena_core::SearchOptions {
        pattern,
        path: validated.to_string_lossy().to_string(),
        glob: None,
        case_sensitive: false,
        max_results: Some(50),
        context_lines: Some(2),
    };
    let result = athena_core::search_code(&options)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
