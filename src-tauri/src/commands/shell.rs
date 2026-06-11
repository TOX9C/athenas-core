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
///
/// Returns an error if the shell is not one of the supported shells. Callers
/// should not inject a fallback script for unsupported shells — the script
/// syntax is shell-specific and a mismatched injection will break the shell.
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
#[tauri::command]
pub fn shell_integration_strip(data: String) -> String {
    athena_core::shell_integration::strip_osc633(&data)
}
