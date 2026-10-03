use super::{caps, CommandError};
use crate::state::AppState;
use tauri::State;

/// Parse OSC 633 sequences from terminal output data.
#[tauri::command]
pub async fn shell_integration_parse(
    state: State<'_, AppState>,
    data: String,
) -> Result<String, String> {
    if data.len() > caps::MAX_DATA_BYTES {
        return Err(format!(
            "data too large: {} > {}",
            data.len(),
            caps::MAX_DATA_BYTES
        ));
    }
    let shell_integration_parser = state.shell_integration_parser.clone();
    tokio::task::spawn_blocking(move || {
        let mut parser = shell_integration_parser.lock();
        let sequences = parser.feed(&data);
        serde_json::to_string(&sequences).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get the shell integration script for the specified shell (bash, zsh, fish).
#[tauri::command]
pub fn shell_integration_script(shell: String) -> Result<String, CommandError> {
    athena_core::shell_integration::get_shell_integration_script(&shell)
        .map_err(|e| CommandError::InvalidInput(e.to_string()))
}

/// Check whether the specified shell supports shell integration.
#[tauri::command]
pub fn shell_integration_compatible(shell: String) -> bool {
    athena_core::shell_integration::is_shell_integration_compatible(&shell)
}

/// Strip OSC 633 sequences from terminal output data.
///
/// Async: `strip_osc633` is a linear scan over potentially large payloads
/// (whole paste buffers). In Tauri 2 sync commands run on the main thread,
/// so the scan is dispatched via `spawn_blocking` to keep the UI off the
/// parse path.
#[tauri::command]
pub async fn shell_integration_strip(data: String) -> String {
    tokio::task::spawn_blocking(move || athena_core::shell_integration::strip_osc633(&data))
        .await
        .unwrap_or_default()
}
