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

/// Get the accumulated output history for a PTY session.
/// Returns the current grid state as a JSON array of rows with cell characters.
#[tauri::command]
pub fn get_pane_history(state: State<'_, AppState>, pane_id: String) -> Result<String, String> {
    let lines = state.output_buffer.get_output(&pane_id, None);
    serde_json::to_string(&lines).map_err(|e| e.to_string())
}
