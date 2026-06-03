use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Parse OSC 633 sequences from terminal output data.
#[tauri::command]
pub async fn shell_integration_parse(state: State<'_, AppState>, data: String) -> Result<String, CommandError> {
    let shell_integration_parser = state.shell_integration_parser.clone();
    tokio::task::spawn_blocking(move || {
        let mut parser = shell_integration_parser.lock().map_err(|e| CommandError::Internal(e.to_string()))?;
        let sequences = parser.feed(&data);
        serde_json::to_string(&sequences).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(e.to_string()))?
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
