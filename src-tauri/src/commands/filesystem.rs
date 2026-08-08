use super::{caps, validate_path, validate_path_exists, CommandError};
use crate::state::AppState;
use base64::Engine;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

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
        // Atomic write: write to a temp file in the same directory, then rename.
        let temp_path = match validated_clone.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                parent.join(format!(".tmp-write-{}", uuid::Uuid::new_v4()))
            }
            _ => std::env::temp_dir().join(format!(".tmp-write-{}", uuid::Uuid::new_v4())),
        };
        std::fs::write(&temp_path, content_clone)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        std::fs::rename(&temp_path, &validated_clone)
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
