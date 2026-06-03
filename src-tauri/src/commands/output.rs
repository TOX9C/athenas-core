use super::CommandError;
use crate::state::AppState;
use tauri::State;

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
) -> Result<String, CommandError> {
    let options = athena_core::output_buffer::GetOutputOptions {
        limit,
        offset,
        since_line: None,
        since_time: None,
        raw: None,
    };
    let lines = state.output_buffer.get_output(&pane_id, Some(&options));
    serde_json::to_string(&lines).map_err(|e| CommandError::Internal(e.to_string()))
}

/// List all agent pane IDs that have captured output.
#[tauri::command]
pub fn output_buffer_list(state: State<'_, AppState>) -> Result<String, CommandError> {
    let agents = state.output_buffer.get_agent_list();
    serde_json::to_string(&agents).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Clear the output buffer for a specific pane.
#[tauri::command]
pub fn output_buffer_clear(state: State<'_, AppState>, pane_id: String) -> Result<bool, CommandError> {
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
) -> Result<String, CommandError> {
    let options = athena_core::output_buffer::GetOutputOptions {
        limit,
        offset,
        since_line: None,
        since_time: None,
        raw: None,
    };
    let lines = state.output_buffer.get_output(&pane_id, Some(&options));
    serde_json::to_string(&lines).map_err(|e| CommandError::Internal(e.to_string()))
}

/// List all agent panes with captured output (alias for output_buffer_list).
#[tauri::command]
pub fn output_capture_list_agents(state: State<'_, AppState>) -> Result<String, CommandError> {
    let agents = state.output_buffer.get_agent_list();
    serde_json::to_string(&agents).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Get metadata about a pane's output buffer (alias for output_buffer info).
#[tauri::command]
pub fn output_capture_get_info(
    state: State<'_, AppState>,
    pane_id: String,
) -> Result<String, CommandError> {
    match state.output_buffer.get_pane_buffer_info(&pane_id) {
        Some(info) => serde_json::to_string(&info).map_err(|e| CommandError::Internal(e.to_string())),
        None => Ok("null".to_string()),
    }
}

/// Clear an agent pane's captured output (alias for output_buffer_clear).
#[tauri::command]
pub fn output_capture_clear(state: State<'_, AppState>, pane_id: String) -> Result<bool, CommandError> {
    Ok(state.output_buffer.clear_pane_buffer(&pane_id))
}
