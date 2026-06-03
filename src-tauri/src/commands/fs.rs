use super::{validate_path, validate_path_exists, CommandError};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[derive(serde::Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// Read the contents of a file as UTF-8 text.
#[tauri::command]
pub async fn fs_read_file(path: String) -> Result<String, CommandError> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(path_ref)?;
    let validated_clone = validated.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::read_to_string(&validated_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
}

/// List the contents of a directory, sorted with directories first.
#[tauri::command]
pub async fn fs_list_dir(path: String) -> Result<String, CommandError> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(path_ref)?;
    tokio::task::spawn_blocking(move || {
        let mut entries: Vec<DirEntry> = Vec::new();
        let read_dir = std::fs::read_dir(&validated).map_err(|e| CommandError::Internal(e.to_string()))?;
        for entry_result in read_dir {
            let entry = entry_result.map_err(|e| CommandError::Internal(e.to_string()))?;
            let file_type = entry.file_type().map_err(|e| CommandError::Internal(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path().to_string_lossy().to_string();
            let is_dir = file_type.is_dir();
            entries.push(DirEntry { name, path, is_dir });
        }
        entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        serde_json::to_string(&entries).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
}

/// Write content to a file, creating it if it doesn't exist.
#[tauri::command]
pub async fn fs_write_file(path: String, content: String) -> Result<(), CommandError> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path(path_ref)?;
    let content_clone = content.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::write(&validated, content_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Write task failed: {e}")))?
}

/// Check whether a path exists and is within the allowed directory.
#[tauri::command]
pub async fn fs_exists(path: String) -> bool {
    let path_ref = std::path::Path::new(&path);
    validate_path(path_ref).is_ok()
}

/// Read a file and return its contents as a base64-encoded string.
#[tauri::command]
pub async fn fs_read_file_as_base64(path: String) -> Result<String, CommandError> {
    use base64::Engine;
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(path_ref)?;
    let validated_clone = validated.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        std::fs::read(&validated_clone).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))?
    ?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Show a native file/folder open dialog and return the selected path(s).
#[tauri::command]
pub async fn fs_show_open_dialog(
    app_handle: AppHandle,
    title: Option<String>,
    #[allow(unused_variables)] filters: Option<String>,
    multiple: Option<bool>,
    directory: Option<bool>,
) -> Result<String, CommandError> {
    let is_directory = directory.unwrap_or(false);
    let is_multiple = multiple.unwrap_or(false);


    let mut dialog = app_handle.dialog().file();
    if let Some(t) = &title {
        dialog = dialog.set_title(t);
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    match (is_directory, is_multiple) {
        (true, false) => {
            dialog.pick_folder( move |path| {
                let result = path.map(|fp| fp.to_string()).unwrap_or_default();
                let _ = tx.send(result);
            });
        }
        (true, true) => {
            dialog.pick_folders( move |paths| {
                let result = paths
                    .map(|list| list.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default();
                let _ = tx.send(result);
            });
        }
        (false, false) => {
            dialog.pick_file( move |path| {
                let result = path.map(|fp| fp.to_string()).unwrap_or_default();
                let _ = tx.send(result);
            });
        }
        (false, true) => {
            dialog.pick_files( move |paths| {
                let result = paths
                    .map(|list| list.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default();
                let _ = tx.send(result);
            });
        }
    }

    let result = rx.await.map_err(|e| CommandError::Internal(format!("Dialog channel closed: {e}")))?;
    Ok(result)
}

/// Show a native file dialog filtered to image types (png, jpg, jpeg, gif, svg, webp).
#[tauri::command]
pub async fn fs_show_image_dialog(app_handle: AppHandle) -> Result<String, CommandError> {
    let dialog = app_handle
        .dialog()
        .file()
        .set_title("Select Image")
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "svg", "webp"]);

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    dialog.pick_file(move |path| {
        let result = path.map(|fp| fp.to_string()).unwrap_or_default();
        let _ = tx.send(result);
    });

    let result = rx.await.map_err(|e| CommandError::Internal(format!("Dialog channel closed: {e}")))?;
    Ok(result)
}

/// Search files in a directory using ripgrep with the given pattern.
#[tauri::command]
pub async fn fs_search_files(pattern: String, path: String) -> Result<String, CommandError> {
    let options = athena_core::SearchOptions {
        pattern,
        path,
        glob: None,
        case_sensitive: false,
        max_results: Some(50),
        context_lines: Some(2),
    };
    let result = athena_core::search_code(&options).await.map_err(|e| CommandError::Internal(e.to_string()))?;
    serde_json::to_string(&result).map_err(|e| CommandError::Internal(e.to_string()))
}
