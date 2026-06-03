use crate::state::AppState;
use base64::Engine;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

// ── Path validation helpers ──────────────────────────────────────────────────

/// Get the canonicalized workspace root for path sandboxing.
fn get_workspace_root() -> Result<std::path::PathBuf, CommandError> {
    std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("Failed to get workspace root: {}", e)))?
        .canonicalize()
        .map_err(|e| {
            CommandError::Internal(format!("Failed to canonicalize workspace root: {}", e))
        })
}

/// Validate that a path exists and return the cleaned path.
fn validate_path_exists(path: &std::path::Path) -> Result<std::path::PathBuf, CommandError> {
    let root = get_workspace_root()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !path.exists() {
        return Err(CommandError::NotFound(format!(
            "Path does not exist: {}",
            path.display()
        )));
    }
    let canonicalized = path
        .canonicalize()
        .map_err(|e| CommandError::Internal(format!("Failed to canonicalize path: {}", e)))?;
    if !canonicalized.starts_with(&root) {
        return Err(CommandError::PermissionDenied(format!(
            "Path must be within the workspace: {}",
            root.display()
        )));
    }
    Ok(canonicalized)
}

/// Validate a path for write operations (creates parent dirs if needed).
fn validate_path(path: &std::path::Path) -> Result<std::path::PathBuf, CommandError> {
    let root = get_workspace_root()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !path.starts_with(&root) {
        return Err(CommandError::PermissionDenied(format!(
            "Path must be within the workspace: {}",
            root.display()
        )));
    }
    if let Ok(remaining) = path.strip_prefix(&root) {
        for comp in remaining.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                return Err(CommandError::PermissionDenied(
                    "Path escapes the workspace root".to_string(),
                ));
            }
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CommandError::Internal(format!("Failed to create parent directories: {}", e))
        })?;
    }
    Ok(path)
}

/// Build provider config from the persistent store for LLM API calls.
fn build_provider_config_from_store(
    state: &AppState,
) -> Option<athena_core::orchestrator::ProviderConfig> {
    let provider_str = state
        .store
        .get::<String>("llm.provider")
        .ok()
        .flatten()
        .unwrap_or_else(|| "anthropic".to_string());
    let api_key = state
        .store
        .get::<String>("llm.api_key")
        .ok()
        .flatten()
        .or_else(|| {
            keyring::Entry::new("athena", "api_key")
                .ok()
                .and_then(|e| e.get_password().ok())
        })
        .unwrap_or_default();
    let model = state
        .store
        .get::<String>("llm.model")
        .ok()
        .flatten()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    if api_key.is_empty() {
        log::warn!("No API key configured for LLM provider");
        return None;
    }

    let provider = match provider_str.as_str() {
        "anthropic" => athena_core::types::LLMProvider::Anthropic,
        "openai" => athena_core::types::LLMProvider::OpenAI,
        "nvidia_nim" => athena_core::types::LLMProvider::NvidiaNim,
        "lmstudio" => athena_core::types::LLMProvider::Lmstudio,
        _ => {
            log::warn!("Unknown LLM provider: {}", provider_str);
            return None;
        }
    };

    Some(athena_core::orchestrator::ProviderConfig {
        provider,
        api_key,
        model,
        system_prompt: String::new(),
        base_url: None,
    })
}

// ── Structured error type for Tauri commands ────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[allow(dead_code)]
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ── Window commands ──────────────────────────────────────────────────────────

/// Minimize the main application window.
#[tauri::command]
pub fn window_minimize(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.minimize().map_err(|e| e.to_string())
}

/// Maximize or restore the main application window.
#[tauri::command]
pub fn window_maximize(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.maximize().map_err(|e| e.to_string())
}

/// Close the main application window.
#[tauri::command]
pub fn window_close(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.close().map_err(|e| e.to_string())
}

/// Check whether the main window is currently maximized.
#[tauri::command]
pub fn window_is_maximized(app_handle: AppHandle) -> Result<bool, String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.is_maximized().map_err(|e| e.to_string())
}

/// Return the current platform identifier (e.g., `"macos"`, `"linux"`, `"windows"`).
#[tauri::command]
pub fn window_platform() -> String {
    std::env::consts::OS.to_string()
}

/// Return the default shell path for the current platform.
#[tauri::command]
pub fn pty_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "powershell.exe".to_string()
        } else {
            "/bin/zsh".to_string()
        }
    })
}

// ── File system commands ─────────────────────────────────────────────────────

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

#[derive(serde::Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// List the contents of a directory, sorted with directories first.
#[tauri::command]
pub async fn fs_list_dir(path: String) -> Result<String, CommandError> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(path_ref)?;
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
    .map_err(|e| CommandError::Internal(format!("Read task failed: {e}")))??;
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
pub async fn fs_search_files(pattern: String, path: String) -> Result<String, String> {
    let options = athena_core::SearchOptions {
        pattern,
        path,
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

// ── Store commands ───────────────────────────────────────────────────────────

/// Get a value from the persistent key-value store.
#[tauri::command]
pub fn store_get(state: State<'_, AppState>, key: String) -> Result<String, CommandError> {
    state
        .store
        .get::<String>(&key)
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Key '{}' not found", key)))
}

/// Set a value in the persistent key-value store.
#[tauri::command]
pub async fn store_set(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    state
        .store
        .set(&key, &value)
        .await
        .map_err(|e| e.to_string())
}

/// Check whether a key exists in the persistent key-value store.
#[tauri::command]
pub fn store_has(state: State<'_, AppState>, key: String) -> bool {
    state.store.has(&key)
}

/// Delete a key from the persistent key-value store.
#[tauri::command]
pub async fn store_delete(state: State<'_, AppState>, key: String) -> Result<(), String> {
    state.store.delete(&key).await.map_err(|e| e.to_string())
}

// ── Session commands ─────────────────────────────────────────────────────────

/// Create a new chat session and return its JSON representation.
#[tauri::command]
pub async fn session_create(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<String, String> {
    let session = state
        .session_store
        .create_session(title.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&session).map_err(|e| e.to_string())
}

/// Get a chat session by its ID.
#[tauri::command]
pub async fn session_get(state: State<'_, AppState>, id: String) -> Result<String, CommandError> {
    let session = state
        .session_store
        .get_session(&id)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match session {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
        None => Err(CommandError::NotFound(format!(
            "Session '{}' not found",
            id
        ))),
    }
}

/// List all chat sessions with summary information (id, title, message count, etc.).
#[tauri::command]
pub async fn session_list(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state
        .session_store
        .list_sessions()
        .await
        .map_err(|e| e.to_string())?;
    let mut json = Vec::new();
    for item in &sessions {
        json.push(serde_json::json!({
            "id": item.id,
            "title": item.title,
            "createdAt": item.created_at,
            "updatedAt": item.updated_at,
            "messageCount": item.message_count,
            "lastMessagePreview": item.last_message_preview
        }));
    }
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

/// Delete a chat session by its ID.
#[tauri::command]
pub async fn session_delete(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .session_store
        .delete_session(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("deleted".to_string())
}

/// Update a chat session's title and/or messages.
#[tauri::command]
pub async fn session_update(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    messages: Option<String>,
) -> Result<String, CommandError> {
    let parsed_messages: Option<Vec<athena_store::SessionMessage>> = match messages {
        Some(json) => Some(
            serde_json::from_str(&json)
                .map_err(|e| CommandError::InvalidInput(format!("Invalid messages JSON: {}", e)))?,
        ),
        None => None,
    };
    let session = state
        .session_store
        .update_session(&id, title.as_deref(), parsed_messages)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match session {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
        None => Err(CommandError::NotFound(format!(
            "Session '{}' not found",
            id
        ))),
    }
}

/// Add a message to an existing chat session.
#[tauri::command]
pub async fn session_add_message(
    state: State<'_, AppState>,
    session_id: String,
    role: String,
    content: String,
    is_error: Option<bool>,
    image_refs: Option<String>,
) -> Result<String, String> {
    let mut session = state
        .session_store
        .get_session(&session_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Session not found".to_string())?;

    let parsed_refs: Option<Vec<athena_store::ImageRef>> = match image_refs {
        Some(json) => Some(serde_json::from_str(&json).map_err(|e| e.to_string())?),
        None => None,
    };

    let message_role = match role.as_str() {
        "user" => athena_store::MessageRole::User,
        _ => athena_store::MessageRole::Athena,
    };

    let msg = athena_store::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: message_role,
        content,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        is_error,
        image_refs: parsed_refs,
    };

    session.messages.push(msg);
    let updated = state
        .session_store
        .update_session(&session_id, None, Some(session.messages))
        .await
        .map_err(|e| e.to_string())?;
    match updated {
        Some(s) => serde_json::to_string(&s).map_err(|e| e.to_string()),
        None => Err("Failed to update session".to_string()),
    }
}

// ── PTY commands ─────────────────────────────────────────────────────────────

/// Spawn a new PTY session with the given ID, working directory, and shell.
/// After spawning, starts a background tokio task that reads PTY output
/// and emits `terminal:data` events to the frontend.
#[tauri::command]
pub async fn pty_spawn(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<(), String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);
    log::info!(
        "pty_spawn requested: id={} cwd={} shell={} cols={} rows={}",
        id,
        cwd,
        shell,
        cols,
        rows
    );
    let session_manager = state.session_manager.lock().await;
    let session_result = session_manager
        .spawn(id.clone(), &shell, &cwd, cols, rows)
        .await;
    drop(session_manager);

    match session_result {
        Ok(session) => {
            let _session_id = id.clone();
            let app_handle = match state.app_handle.lock() {
                Ok(g) => g.clone(),
                Err(e) => {
                    log::error!("app_handle lock poisoned in pty_spawn: {}", e);
                    return Ok(());
                }
            };

            if let Some(handle) = app_handle {
                let session_id_for_loop = id.clone();
                tokio::spawn(async move {
                    pty_read_loop(handle, session_id_for_loop, session).await;
                });
            }

            log::info!(
                "PTY session spawned: id={} cwd={} shell={} cols={} rows={}",
                id,
                cwd,
                shell,
                cols,
                rows
            );
            Ok(())
        }
        Err(e) => {
            log::error!(
                "Failed to spawn PTY session: id={} cwd={} shell={} cols={} rows={} error={}",
                id,
                cwd,
                shell,
                cols,
                rows,
                e
            );
            Err(e.to_string())
        }
    }
}

/// Background task that reads PTY output and emits Tauri events.
///
/// Fans out to two parallel event streams:
/// - `pty:raw` — base64-encoded raw PTY bytes, consumed by the xterm.js
///   frontend (which has its own ANSI parser). Emitted on every successful
///   read regardless of whether the grid state changed.
/// - `terminal:data` — parsed cell deltas, consumed by the legacy
///   cell-grid frontend. Emitted only when the grid actually changed.
async fn pty_read_loop(
    app_handle: tauri::AppHandle,
    session_id: String,
    session: std::sync::Arc<athena_terminal::session::TerminalSession>,
) {
    log::info!("pty_read_loop[{}]: starting", session_id);
    let mut did_emit_ready = false;

    let mut buf = vec![0u8; 4096];
    loop {
        // Step 1: pull raw bytes from the PTY. `0` means EAGAIN on a
        // non-blocking fd — sleep briefly and loop.
        let n = match session.read_bytes(&mut buf).await {
            Ok(0) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                continue;
            }
            Ok(n) => n,
            Err(e) => {
                log::warn!("PTY read error for {}: {}", session_id, e);
                if e.kind() == std::io::ErrorKind::BrokenPipe
                    || e.kind() == std::io::ErrorKind::InvalidData
                {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                continue;
            }
        };

        log::debug!("pty_read_loop[{}]: read {} bytes", session_id, n);

        // Step 2: fan out raw bytes to xterm.js subscribers. Base64 wraps
        // arbitrary bytes (including invalid UTF-8) into a JSON-safe string.
        let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
        let raw_event = serde_json::json!({
            "sessionId": session_id,
            "data": encoded,
        });
        // Serialize to a fully-owned String before calling emit. Passing
        // `&raw_event` (a `&serde_json::Value`) to emit captured a borrow
        // that, across concurrent tokio tasks, was observed to be read
        // after a later task had overwritten the underlying buffer — all
        // listeners then received payloads whose `sessionId` field matched
        // whichever task had last serialized. Owning the String forces
        // serialization to happen on this task, eliminating the race.
        let raw_event_str = match serde_json::to_string(&raw_event) {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "pty_read_loop[{}]: failed to serialize raw_event: {}",
                    session_id,
                    e
                );
                continue;
            }
        };
        if let Err(e) = app_handle.emit("pty:raw", raw_event_str) {
            log::warn!("Failed to emit pty:raw event: {}", e);
        }

        // Step 3: parse the same bytes for the legacy cell-grid frontend.
        // `parse_bytes` returns `None` when no cells changed, in which
        // case we skip the structured event entirely.
        match session.parse_bytes(&buf[..n]).await {
            Ok(Some(update)) => {
                if !did_emit_ready {
                    did_emit_ready = true;
                    session.mark_ready().await;
                    // Clone to an owned String to avoid the same borrow-sharing
                    // race that motivated the pty:raw String-serialize fix.
                    if let Err(e) = app_handle.emit("terminal:ready", session_id.clone()) {
                        log::warn!("Failed to emit terminal:ready event: {}", e);
                    }
                }
                let event_data = serde_json::json!({
                    "sessionId": session_id,
                    "deltas": update.deltas,
                    "cursorRow": update.cursor_row,
                    "cursorCol": update.cursor_col,
                    "rows": update.rows,
                    "cols": update.cols,
                    "cursorVisible": update.cursor_visible,
                });
                let event_data_str = match serde_json::to_string(&event_data) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!(
                            "pty_read_loop[{}]: failed to serialize event_data: {}",
                            session_id,
                            e
                        );
                        continue;
                    }
                };
                if let Err(e) = app_handle.emit("terminal:data", event_data_str) {
                    log::warn!("Failed to emit terminal:data event: {}", e);
                }
            }
            Ok(None) => {}
            Err(e) => {
                log::warn!("PTY parse error for {}: {}", session_id, e);
            }
        }

        // Rate limit: yield after each successful read to prevent CPU spin
        // when commands like `yes` produce infinite output.
        tokio::task::yield_now().await;
    }

    log::info!("PTY read loop exited for session: {}", session_id);
    if let Err(e) = app_handle.emit("terminal:exit", session_id) {
        log::warn!("Failed to emit terminal:exit event: {}", e);
    }
}

/// Write data to a PTY session's stdin.
#[tauri::command]
pub async fn pty_write(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    let data_len = data.len();
    let session_manager = state.session_manager.lock().await;
    let _len = session_manager
        .write(&id, data.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    log::debug!("pty_write: id={} bytes={}", id, data_len);
    drop(session_manager);
    Ok(())
}

/// Kill a PTY session by its ID.
#[tauri::command]
pub async fn pty_kill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.kill(&id).await;
    drop(session_manager);
    result.map_err(|e| e.to_string())
}

/// Resize a PTY session's terminal dimensions.
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    log::info!(
        "pty_resize requested: id={} cols={} rows={}",
        id,
        cols,
        rows
    );
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.resize(&id, cols, rows).await;
    drop(session_manager);
    result.map_err(|e| e.to_string())
}

/// Get the accumulated output history for a PTY session.
/// Returns the current grid state as a JSON array of rows with cell characters.
#[tauri::command]
pub async fn pty_get_history(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        let grid = s.grid.lock().await;
        let mut rows_json = Vec::new();
        for row in &grid.rows {
            let chars: Vec<String> = row.iter().map(|c| c.c.to_string()).collect();
            rows_json.push(serde_json::json!({ "cells": chars }));
        }
        return serde_json::to_string(&serde_json::json!({
            "rows": rows_json,
            "cursor_row": grid.cursor.row,
            "cursor_col": grid.cursor.col,
        }))
        .map_err(|e| e.to_string());
    }
    Ok("null".to_string())
}

/// Check whether a PTY session with the given ID exists.
#[tauri::command]
pub async fn pty_has_session(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.has_session(&id).await;
    drop(session_manager);
    Ok(result)
}

/// Check whether a PTY session's shell prompt is visible (ready).
/// Returns true only when the session status is Ready (shell has started).
#[tauri::command]
pub async fn pty_is_ready(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let session_manager = state.session_manager.lock().await;
    let result = match session_manager.get_session(&id).await {
        Some(session) => {
            let status = session.status.lock().await;
            *status == athena_terminal::session::PtyStatus::Ready
        }
        None => false,
    };
    drop(session_manager);
    Ok(result)
}

/// Get the working directory of a PTY session, if known.
#[tauri::command]
pub async fn pty_get_cwd(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        Ok(Some(s.cwd.clone()))
    } else {
        Ok(None)
    }
}

/// Spawn a new PTY session with the agent command to execute after startup.
/// The `agent_cmd` is executed in the shell after the PTY is set up.
#[tauri::command]
pub async fn pty_spawn_agent(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    agent_cmd: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<(), String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);
    let session_manager = state.session_manager.lock().await;
    let session_result = session_manager
        .spawn(id.clone(), &shell, &cwd, cols, rows)
        .await;
    drop(session_manager);

    match session_result {
        Ok(session) => {
            let _session_id = id.clone();
            let app_handle = match state.app_handle.lock() {
                Ok(g) => g.clone(),
                Err(e) => {
                    log::error!("app_handle lock poisoned in pty_spawn_agent: {}", e);
                    return Ok(());
                }
            };

            // Write the agent command to the PTY
            if let Err(e) = session.write(agent_cmd.as_bytes()).await {
                log::error!("Failed to write agent command to PTY: {}", e);
                return Err(e.to_string());
            }

            if let Some(handle) = app_handle {
                let session_id_for_loop = id.clone();
                tokio::spawn(async move {
                    pty_read_loop(handle, session_id_for_loop, session).await;
                });
            }

            log::info!("PTY agent session spawned: id={}", id);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to spawn PTY agent session: {}", e);
            Err(e.to_string())
        }
    }
}

// ── Athena / Orchestrator commands ───────────────────────────────────────────

/// Send a text message to the configured LLM provider and return the response.
#[tauri::command]
pub async fn athena_chat(state: State<'_, AppState>, message: String) -> Result<String, String> {
    let orchestrator = state.orchestrator.lock().await;
    if let Some(config) = build_provider_config_from_store(&state) {
        orchestrator.set_provider_config(config);
    }
    orchestrator
        .send_message(message, None)
        .await
        .map_err(|e| e.to_string())
}

/// Send a text message to the LLM provider, associating it with a specific session.
#[tauri::command]
pub async fn athena_chat_with_session(
    state: State<'_, AppState>,
    message: String,
    session_id: String,
) -> Result<String, String> {
    let orchestrator = state.orchestrator.lock().await;
    if let Some(config) = build_provider_config_from_store(&state) {
        orchestrator.set_provider_config(config);
    }
    orchestrator.set_current_session_id(session_id);
    orchestrator
        .send_message(message, None)
        .await
        .map_err(|e| e.to_string())
}

/// Send a message with image attachments to the LLM provider.
#[tauri::command]
pub async fn athena_chat_with_images(
    state: State<'_, AppState>,
    message: String,
    images: String,
) -> Result<String, String> {
    let image_data: Vec<athena_core::types::ImageData> =
        serde_json::from_str(&images).map_err(|e| e.to_string())?;
    let orchestrator = state.orchestrator.lock().await;
    if let Some(config) = build_provider_config_from_store(&state) {
        orchestrator.set_provider_config(config);
    }
    orchestrator
        .send_message(message, Some(image_data))
        .await
        .map_err(|e| e.to_string())
}

/// Clear all conversation history from the orchestrator.
#[tauri::command]
pub async fn athena_clear_context(state: State<'_, AppState>) -> Result<(), String> {
    let orchestrator = state.orchestrator.lock().await;
    orchestrator.clear_context();
    Ok(())
}

/// Set the conversation history from a list of session entries.
#[tauri::command]
pub async fn athena_set_session_context(
    state: State<'_, AppState>,
    history: String,
) -> Result<(), String> {
    let entries: Vec<athena_core::types::SessionHistoryEntry> =
        serde_json::from_str(&history).map_err(|e| e.to_string())?;
    let orchestrator = state.orchestrator.lock().await;
    orchestrator.set_session_context(entries);
    Ok(())
}

/// Provide an answer to a pending user question from the orchestrator.
#[tauri::command]
pub fn athena_user_answer(
    state: State<'_, AppState>,
    request_id: String,
    answer: String,
) -> Result<bool, String> {
    let mut map = state
        .pending_questions
        .lock()
        .map_err(|e| format!("pending_questions lock poisoned: {}", e))?;
    if let Some(tx) = map.remove(&request_id) {
        let _ = tx.send(answer);
        Ok(true)
    } else {
        log::warn!("no pending question found for request_id: {}", request_id);
        Ok(false)
    }
}

/// Store an API key securely in the OS keychain.
#[tauri::command]
pub fn store_api_key(key: String) -> Result<(), String> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .set_password(&key)
        .map_err(|e| format!("Failed to store API key in keyring: {}", e))
}

/// Clear the API key from the OS keychain.
#[tauri::command]
pub fn clear_api_key() -> Result<(), String> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .delete_credential()
        .or_else(|e| {
            if matches!(e, keyring::Error::NoEntry) {
                Ok(())
            } else {
                Err(e)
            }
        })
        .map_err(|e| format!("Failed to clear API key from keyring: {}", e))
}

// ── Output buffer commands ───────────────────────────────────────────────────

/// Append data to an output buffer for a specific pane.
#[tauri::command]
pub fn output_buffer_append(
    state: State<'_, AppState>,
    pane_id: String,
    data: String,
    agent_type: Option<String>,
) {
    state
        .output_buffer
        .append_output(&pane_id, &data, agent_type.as_deref());
}

/// Get output lines from a pane's buffer with optional pagination.
#[tauri::command]
pub fn output_buffer_get(
    state: State<'_, AppState>,
    pane_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::output_buffer::GetOutputOptions {
        limit,
        offset,
        since_line: None,
        since_time: None,
        raw: None,
    };
    let lines = state.output_buffer.get_output(&pane_id, Some(&options));
    serde_json::to_string(&lines).map_err(|e| e.to_string())
}

/// List all agent pane IDs that have captured output.
#[tauri::command]
pub fn output_buffer_list(state: State<'_, AppState>) -> Result<String, String> {
    let agents = state.output_buffer.get_agent_list();
    serde_json::to_string(&agents).map_err(|e| e.to_string())
}

/// Clear the output buffer for a specific pane.
#[tauri::command]
pub fn output_buffer_clear(state: State<'_, AppState>, pane_id: String) -> Result<bool, String> {
    Ok(state.output_buffer.clear_pane_buffer(&pane_id))
}

// ── Output capture commands (aliases matching Electron preload API) ──────────

/// Read captured output from an agent pane (alias for output_buffer_get).
#[tauri::command]
pub fn output_capture_read(
    state: State<'_, AppState>,
    pane_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::output_buffer::GetOutputOptions {
        limit,
        offset,
        since_line: None,
        since_time: None,
        raw: None,
    };
    let lines = state.output_buffer.get_output(&pane_id, Some(&options));
    serde_json::to_string(&lines).map_err(|e| e.to_string())
}

/// List all agent panes with captured output (alias for output_buffer_list).
#[tauri::command]
pub fn output_capture_list_agents(state: State<'_, AppState>) -> Result<String, String> {
    let agents = state.output_buffer.get_agent_list();
    serde_json::to_string(&agents).map_err(|e| e.to_string())
}

/// Get metadata about a pane's output buffer (alias for output_buffer info).
#[tauri::command]
pub fn output_capture_get_info(
    state: State<'_, AppState>,
    pane_id: String,
) -> Result<String, String> {
    match state.output_buffer.get_pane_buffer_info(&pane_id) {
        Some(info) => serde_json::to_string(&info).map_err(|e| e.to_string()),
        None => Ok("null".to_string()),
    }
}

/// Clear an agent pane's captured output (alias for output_buffer_clear).
#[tauri::command]
pub fn output_capture_clear(state: State<'_, AppState>, pane_id: String) -> Result<bool, String> {
    Ok(state.output_buffer.clear_pane_buffer(&pane_id))
}

// ── Notification commands ────────────────────────────────────────────────────

/// Push a new notification to the notification service.
#[tauri::command]
pub fn notification_push(
    state: State<'_, AppState>,
    title: String,
    message: String,
    level: Option<String>,
) -> Result<String, String> {
    let notif_type = match level.as_deref() {
        Some("warning") => athena_core::notification::NotificationType::Warning,
        Some("error") => athena_core::notification::NotificationType::Error,
        Some("success") => athena_core::notification::NotificationType::Success,
        Some("needs_input") => athena_core::notification::NotificationType::NeedsInput,
        Some("task_complete") => athena_core::notification::NotificationType::TaskComplete,
        Some("task_error") => athena_core::notification::NotificationType::TaskError,
        _ => athena_core::notification::NotificationType::Info,
    };
    let event = athena_core::notification::NotificationEvent {
        r#type: notif_type,
        title,
        message,
        source: "command".to_string(),
        agent_id: None,
        data: None,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        metadata: None,
        actions: None,
        request_id: None,
    };
    let record = state.notification_service.push_notification(event);
    serde_json::to_string(&record).map_err(|e| e.to_string())
}

/// Get the notification history with optional filtering.
#[tauri::command]
pub fn notification_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::notification::HistoryOptions {
        limit,
        unread_only: None,
        r#type: None,
        source: None,
    };
    let history = state.notification_service.get_history(Some(&options));
    serde_json::to_string(&history).map_err(|e| e.to_string())
}

/// Get the count of unread notifications.
#[tauri::command]
pub fn notification_count(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.notification_service.get_unread_count())
}

/// Mark a specific notification as read.
#[tauri::command]
pub fn notification_mark_read(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .mark_read(&notification_id)
        .map_err(|e| e.to_string())
}

/// Mark all notifications as read. Returns the number of notifications marked.
#[tauri::command]
pub fn notification_mark_all_read(state: State<'_, AppState>) -> usize {
    state.notification_service.mark_all_read()
}

/// Dismiss (remove) a notification from the history.
#[tauri::command]
pub fn notification_dismiss(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .dismiss(&notification_id)
        .map_err(|e| e.to_string())
}

/// Clear all notifications from the history. Returns the number cleared.
#[tauri::command]
pub fn notification_clear_all(state: State<'_, AppState>) -> usize {
    state.notification_service.clear_all()
}

/// Get a breakdown of notification counts by type.
#[tauri::command]
pub fn notification_counts(state: State<'_, AppState>) -> Result<String, String> {
    let counts = state.notification_service.get_counts();
    serde_json::to_string(&counts).map_err(|e| e.to_string())
}

// ── Plan manager commands ────────────────────────────────────────────────────

/// Create a new execution plan with a goal, reasoning, and steps.
#[tauri::command]
pub fn plan_create(
    state: State<'_, AppState>,
    goal: String,
    reasoning: String,
    steps: String,
) -> Result<String, String> {
    let step_list: Vec<athena_core::plan_manager::PlanStepInput> =
        serde_json::from_str(&steps).map_err(|e| e.to_string())?;
    let input = athena_core::plan_manager::PlanInput {
        goal,
        reasoning,
        steps: step_list,
    };
    let plan = state
        .plan_manager
        .set_active_plan(input)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&plan).map_err(|e| e.to_string())
}

/// Get the currently active plan, if any.
#[tauri::command]
pub fn plan_get(state: State<'_, AppState>) -> Result<String, String> {
    let plan = state.plan_manager.get_active_plan();
    serde_json::to_string(&plan).map_err(|e| e.to_string())
}

/// Update the status of a specific step in the active plan.
#[tauri::command]
pub fn plan_update_step(
    state: State<'_, AppState>,
    step_id: String,
    status: String,
    pane_id: Option<String>,
) -> Result<bool, String> {
    let step_status = match status.as_str() {
        "pending" => athena_core::plan_manager::StepStatus::Pending,
        "in_progress" => athena_core::plan_manager::StepStatus::InProgress,
        "completed" => athena_core::plan_manager::StepStatus::Completed,
        "failed" => athena_core::plan_manager::StepStatus::Failed,
        _ => return Err("Invalid status".to_string()),
    };
    state
        .plan_manager
        .update_step_status(&step_id, step_status, pane_id.as_deref())
        .map_err(|e| e.to_string())
}

// ── Agent comms commands ─────────────────────────────────────────────────────

/// Get the agent comms session token for authenticating agent connections.
#[tauri::command]
pub fn agent_comms_token(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.agent_comms.get_comms_token().to_string())
}

/// Get a list of all active agent sessions.
#[tauri::command]
pub fn agent_comms_sessions(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.agent_comms.get_agent_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Send a message to a specific agent via the agent comms channel.
#[tauri::command]
pub fn agent_comms_send(
    state: State<'_, AppState>,
    agent_id: String,
    method: String,
    params: String,
) -> Result<bool, String> {
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    state
        .agent_comms
        .send_to_agent(&agent_id, &method, &params_json)
        .map_err(|e| e.to_string())
}

// ── Agents commands (matching Electron preload API naming) ───────────────────

/// List all connected agent sessions (alias for agent_comms_sessions).
#[tauri::command]
pub fn agents_list(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.agent_comms.get_agent_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Get the status of a specific agent by its ID.
#[tauri::command]
pub fn agent_get_status(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<String, CommandError> {
    let sessions = state.agent_comms.get_agent_sessions();
    let session = sessions
        .iter()
        .find(|s| s.agent_id == agent_id)
        .ok_or_else(|| CommandError::NotFound(format!("Agent '{}' not found", agent_id)))?;
    serde_json::to_string(&session).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Respond to a pending input request from an agent.
#[tauri::command]
pub fn agent_respond_input(
    state: State<'_, AppState>,
    request_id: String,
    response: String,
) -> Result<bool, String> {
    state
        .agent_comms
        .respond_to_input_request(&request_id, &response)
        .map_err(|e| e.to_string())
}

/// Cancel a pending input request from an agent.
#[tauri::command]
pub fn agent_cancel_input(state: State<'_, AppState>, request_id: String) -> Result<bool, String> {
    state
        .agent_comms
        .cancel_input_request(&request_id)
        .map_err(|e| e.to_string())
}

/// Send a message to a specific agent (alias for agent_comms_send).
#[tauri::command]
pub fn agent_send_message(
    state: State<'_, AppState>,
    agent_id: String,
    method: String,
    params: String,
) -> Result<bool, String> {
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    state
        .agent_comms
        .send_to_agent(&agent_id, &method, &params_json)
        .map_err(|e| e.to_string())
}

/// Disconnect an agent by its ID.
#[tauri::command]
pub fn agent_disconnect(state: State<'_, AppState>, agent_id: String) -> Result<bool, String> {
    state
        .agent_comms
        .disconnect_agent(&agent_id)
        .map_err(|e| e.to_string())
}

/// Get the agent comms session token (alias for agent_comms_token).
#[tauri::command]
pub fn agent_get_token(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.agent_comms.get_comms_token().to_string())
}

// ── Search commands ──────────────────────────────────────────────────────────

/// Search the codebase for a pattern using ripgrep.
#[tauri::command]
pub async fn search_code(pattern: String, path: String) -> Result<String, String> {
    let options = athena_core::SearchOptions {
        pattern,
        path,
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

/// Search the codebase using ripgrep (alias for search_code).
#[tauri::command]
pub async fn search_ripgrep(pattern: String, path: String) -> Result<String, String> {
    search_code(pattern, path).await
}

// ── MCP server commands ──────────────────────────────────────────────────────

/// Initialize the MCP server on the given port.
#[tauri::command]
pub async fn mcp_init(state: State<'_, AppState>, port: u16) -> Result<(), String> {
    let mut server = state.mcp_server.lock().await;
    server.init(port).map_err(|e| e.to_string())
}

/// Shut down the MCP server.
#[tauri::command]
pub async fn mcp_shutdown(state: State<'_, AppState>) -> Result<(), String> {
    let mut server = state.mcp_server.lock().await;
    server.shutdown();
    Ok(())
}

/// Handle a JSON-RPC request through the MCP server.
#[tauri::command]
pub async fn mcp_handle_request(
    state: State<'_, AppState>,
    request: String,
) -> Result<String, String> {
    let server = state.mcp_server.lock().await;
    let req =
        athena_core::mcp::McpServer::parse_request(&request).ok_or("Invalid JSON-RPC request")?;
    let resp = server.handle_request(&req).await;
    Ok(athena_core::mcp::McpServer::serialize_response(&resp))
}

/// Broadcast a notification to all connected MCP clients.
#[tauri::command]
pub async fn mcp_broadcast(
    state: State<'_, AppState>,
    method: String,
    params: String,
) -> Result<(), String> {
    let server = state.mcp_server.lock().await;
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    server.broadcast_notification(&method, &params_json);
    Ok(())
}

/// List all tools exposed by the MCP server.
#[tauri::command]
pub fn mcp_tools() -> Result<String, String> {
    let tools = athena_core::mcp::get_tools();
    serde_json::to_string(&tools).map_err(|e| e.to_string())
}

// ── Swarm commands ───────────────────────────────────────────────────────────

/// Read the current swarm state from the given directory.
#[tauri::command]
pub async fn swarm_read_state(state: State<'_, AppState>, dir: String) -> Result<String, String> {
    let coordinator = state.swarm_coordinator.lock().await;
    let result = coordinator
        .read_state(&dir)
        .await
        .map_err(|e| e.to_string())?;
    match result {
        Some(s) => serde_json::to_string(&s).map_err(|e| e.to_string()),
        None => Ok("null".to_string()),
    }
}

/// Send a message from one swarm agent to another via the mailbox system.
#[tauri::command]
pub async fn swarm_send_message(
    state: State<'_, AppState>,
    dir: String,
    from: String,
    to: String,
    content: String,
) -> Result<(), String> {
    let coordinator = state.swarm_coordinator.lock().await;
    coordinator
        .send_message(&dir, &from, &to, &content)
        .await
        .map_err(|e| e.to_string())
}

/// Read all messages from a swarm agent's mailbox.
#[tauri::command]
pub async fn swarm_read_mailbox(
    state: State<'_, AppState>,
    dir: String,
    agent_id: String,
) -> Result<String, String> {
    let coordinator = state.swarm_coordinator.lock().await;
    let messages = coordinator
        .read_mailbox(&dir, &agent_id)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&messages).map_err(|e| e.to_string())
}

// ── Shell integration commands ───────────────────────────────────────────────

/// Parse OSC 633 sequences from terminal output data.
#[tauri::command]
pub async fn shell_integration_parse(
    state: State<'_, AppState>,
    data: String,
) -> Result<String, String> {
    let shell_integration_parser = state.shell_integration_parser.clone();
    tokio::task::spawn_blocking(move || {
        let mut parser = shell_integration_parser.lock().map_err(|e| e.to_string())?;
        let sequences = parser.feed(&data);
        serde_json::to_string(&sequences).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get the shell integration script for the specified shell (bash, zsh, fish).
#[tauri::command]
pub fn shell_integration_script(shell: String) -> String {
    athena_core::shell_integration::get_shell_integration_script(&shell)
}

/// Check whether the specified shell supports shell integration.
#[tauri::command]
pub fn shell_integration_compatible(shell: String) -> bool {
    athena_core::shell_integration::is_shell_integration_compatible(&shell)
}

/// Strip OSC 633 sequences from terminal output data.
#[tauri::command]
pub fn shell_integration_strip(data: String) -> String {
    athena_core::shell_integration::strip_osc633(&data)
}

// ── Tool executor commands ───────────────────────────────────────────────────

/// Execute a built-in tool by name with the given arguments.
#[tauri::command]
pub async fn tool_execute(
    state: State<'_, AppState>,
    tool_name: String,
    arguments: String,
) -> Result<String, String> {
    let tool_executor = state.tool_executor.clone();
    tokio::task::spawn_blocking(move || {
        let executor = tool_executor.lock().map_err(|e| e.to_string())?;
        let input: athena_core::tool_executor::ToolInput =
            serde_json::from_str(&arguments).map_err(|e| e.to_string())?;
        let result = executor
            .execute_tool_call(&tool_name, &input)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List all built-in tools available in the tool executor.
#[tauri::command]
pub fn tool_list() -> Result<String, String> {
    let tools = athena_core::tool_executor::orchestrator_tools();
    serde_json::to_string(&tools).map_err(|e| e.to_string())
}

/// Get the OpenAI-compatible tool schemas for all built-in tools.
#[tauri::command]
pub fn tool_openai_schema() -> Result<String, String> {
    let schemas = athena_core::tool_executor::to_openai_tools();
    serde_json::to_string(&schemas).map_err(|e| e.to_string())
}

// ── Browser commands ─────────────────────────────────────────────────────────

/// Open a browser window with the given URL.
#[tauri::command]
pub fn browser_show(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    state
        .browser_manager
        .open_browser(id, &url)
        .map_err(|e| e.to_string())
}

/// Close a browser window by its ID.
#[tauri::command]
pub fn browser_hide(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .browser_manager
        .close_browser(&id)
        .map_err(|e| e.to_string())
}

/// Navigate a browser window to a new URL.
#[tauri::command]
pub fn browser_navigate(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    state
        .browser_manager
        .navigate(&id, &url)
        .map_err(|e| e.to_string())
}

/// Navigate a browser window back one page.
#[tauri::command]
pub fn browser_back(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .browser_manager
        .go_back(&id)
        .map_err(|e| e.to_string())
}

/// Navigate a browser window forward one page.
#[tauri::command]
pub fn browser_forward(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .browser_manager
        .go_forward(&id)
        .map_err(|e| e.to_string())
}

/// Reload a browser window.
#[tauri::command]
pub fn browser_reload(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.browser_manager.reload(&id).map_err(|e| e.to_string())
}

// ── Plugin commands ──────────────────────────────────────────────────────────

/// List all registered plugins.
#[tauri::command]
pub fn plugin_list(state: State<'_, AppState>) -> Result<String, String> {
    let plugins = state.plugin_manager.list_plugins();
    serde_json::to_string(&plugins).map_err(|e| e.to_string())
}

/// Get detailed information about a specific plugin.
#[tauri::command]
pub fn plugin_get(state: State<'_, AppState>, plugin_id: String) -> Result<String, CommandError> {
    let plugin = state
        .plugin_manager
        .get_plugin_info(&plugin_id)
        .ok_or_else(|| CommandError::NotFound(format!("Plugin '{}' not found", plugin_id)))?;
    serde_json::to_string(&plugin).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Register a new plugin with the plugin manager.
#[tauri::command]
pub fn plugin_register(
    state: State<'_, AppState>,
    plugin_id: String,
    name: String,
    version: String,
) -> Result<String, String> {
    let manifest = athena_plugins::PluginManifest {
        id: plugin_id,
        name,
        version,
        description: String::new(),
        author: String::new(),
        permissions: vec![],
        mcp_config: None,
        min_athena_version: None,
        capabilities: vec![],
        tools: vec![],
        subscribes_to: None,
        config: None,
        install: None,
    };
    let id = state
        .plugin_manager
        .register_plugin(manifest)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Unregister a plugin by its ID.
#[tauri::command]
pub fn plugin_unregister(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Enable a plugin by its ID.
#[tauri::command]
pub fn plugin_enable(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .enable_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Disable a plugin by its ID.
#[tauri::command]
pub fn plugin_disable(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .disable_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Get the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_get_config(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<String, CommandError> {
    let config = state
        .plugin_manager
        .get_plugin_config(&plugin_id)
        .ok_or_else(|| CommandError::NotFound(format!("Plugin '{}' not found", plugin_id)))?;
    serde_json::to_string(&config).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Set the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_set_config(
    state: State<'_, AppState>,
    plugin_id: String,
    config: String,
) -> Result<(), String> {
    let config_value: serde_json::Value =
        serde_json::from_str(&config).map_err(|e| e.to_string())?;
    state
        .plugin_manager
        .set_plugin_config(&plugin_id, &config_value)
        .map_err(|e| e.to_string())
}

/// Record an error for a specific plugin.
#[tauri::command]
pub fn plugin_set_error(
    state: State<'_, AppState>,
    plugin_id: String,
    error: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .set_plugin_error(&plugin_id, &error)
        .map_err(|e| e.to_string())
}

// ── Plugin host commands ─────────────────────────────────────────────────────

/// List all active plugin host sessions.
#[tauri::command]
pub fn plugin_host_list_sessions(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.plugin_manager.list_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Get details about a specific plugin host session.
#[tauri::command]
pub fn plugin_host_get_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, CommandError> {
    let session = state
        .plugin_manager
        .get_session(&session_id)
        .ok_or_else(|| CommandError::NotFound(format!("Session '{}' not found", session_id)))?;
    serde_json::to_string(&session).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Emit a plugin event with the given type and data.
#[tauri::command]
pub fn plugin_host_emit_event(
    state: State<'_, AppState>,
    event_type: String,
    data: String,
) -> Result<String, String> {
    let parsed_type: athena_plugins::PluginEventType =
        serde_json::from_str(&format!("\"{}\"", event_type.to_lowercase()))
            .map_err(|e| format!("Invalid event type '{}': {}", event_type, e))?;
    let payload: athena_plugins::PluginEventPayload =
        serde_json::from_str(&data).map_err(|e| format!("Invalid event payload: {}", e))?;
    let source = athena_plugins::PluginEventSource {
        session_id: String::new(),
        pane_id: None,
        agent_type: String::new(),
        agent_id: None,
    };
    let event = state
        .plugin_manager
        .emit_plugin_event(parsed_type, source, payload);
    serde_json::to_string(&event).map_err(|e| e.to_string())
}

/// Subscribe a session to specific plugin event types.
#[tauri::command]
pub fn plugin_host_subscribe(
    state: State<'_, AppState>,
    session_id: String,
    event_types: String,
) -> Result<(), String> {
    let types: Vec<athena_plugins::PluginEventType> =
        serde_json::from_str(&event_types).map_err(|e| format!("Invalid event types: {}", e))?;
    state
        .plugin_manager
        .subscribe_session(&session_id, &types)
        .map_err(|e| e.to_string())
}

/// Update the status of a plugin host session.
#[tauri::command]
pub fn plugin_host_update_status(
    state: State<'_, AppState>,
    session_id: String,
    status: String,
) -> Result<(), String> {
    let parsed_status: athena_plugins::SessionStatus =
        serde_json::from_str(&format!("\"{}\"", status.to_lowercase()))
            .map_err(|e| format!("Invalid session status '{}': {}", status, e))?;
    state
        .plugin_manager
        .update_session_status(&session_id, parsed_status, None)
        .map_err(|e| e.to_string())
}

/// Unregister a plugin host session.
#[tauri::command]
pub fn plugin_host_unregister_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .remove_session(&session_id)
        .map_err(|e| e.to_string())
}

/// Discover plugins in the given directory by scanning for manifest files.
#[tauri::command]
pub fn plugin_host_discover_plugins(
    state: State<'_, AppState>,
    dir: String,
) -> Result<String, String> {
    let results = state
        .plugin_manager
        .discover_plugins(std::path::Path::new(&dir))
        .map_err(|e| e.to_string())?;
    // Convert inner errors to strings since PluginError doesn't implement Serialize
    let serializable: Vec<serde_json::Value> = results
        .into_iter()
        .map(|r| match r {
            Ok(manifest) => serde_json::to_value(manifest)
                .unwrap_or_else(|_| serde_json::json!({"error": "serialization failed"})),
            Err(e) => serde_json::json!({"error": e.to_string()}),
        })
        .collect();
    serde_json::to_string(&serializable).map_err(|e| e.to_string())
}

/// Register and set up a plugin with the given manifest information.
#[tauri::command]
pub fn plugin_host_setup_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    name: String,
    version: String,
) -> Result<String, String> {
    let manifest = athena_plugins::PluginManifest {
        id: plugin_id,
        name,
        version,
        description: String::new(),
        author: String::new(),
        permissions: vec![],
        mcp_config: None,
        min_athena_version: None,
        capabilities: vec![],
        tools: vec![],
        subscribes_to: None,
        config: None,
        install: None,
    };
    let id = state
        .plugin_manager
        .register_plugin(manifest)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Remove a plugin by its ID (alias for plugin_unregister).
#[tauri::command]
pub fn plugin_host_remove_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}
