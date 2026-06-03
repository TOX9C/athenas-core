use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Get the agent comms session token for authenticating agent connections.
#[tauri::command]
pub fn agent_comms_token(state: State<'_, AppState>) -> Result<String, CommandError> {
    Ok(state.agent_comms.get_comms_token().to_string())
}

/// Get a list of all active agent sessions.
#[tauri::command]
pub fn agent_comms_sessions(state: State<'_, AppState>) -> Result<String, CommandError> {
    let sessions = state.agent_comms.get_agent_sessions();
    serde_json::to_string(&sessions).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Send a message to a specific agent via the agent comms channel.
#[tauri::command]
pub fn agent_comms_send(
    state: State<'_, AppState>,
    agent_id: String,
    method: String,
    params: String,
) -> Result<bool, CommandError> {
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| CommandError::InvalidInput(format!("Invalid params JSON: {}", e)))?;
    state
        .agent_comms
        .send_to_agent(&agent_id, &method, &params_json)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

// ── Agents commands (matching Electron preload API naming) ───────────────────

/// List all connected agent sessions (alias for agent_comms_sessions).
#[tauri::command]
pub fn agents_list(state: State<'_, AppState>) -> Result<String, CommandError> {
    let sessions = state.agent_comms.get_agent_sessions();
    serde_json::to_string(&sessions).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Get the status of a specific agent by its ID.
#[tauri::command]
pub fn agent_get_status(state: State<'_, AppState>, agent_id: String) -> Result<String, CommandError> {
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
) -> Result<bool, CommandError> {
    state
        .agent_comms
        .respond_to_input_request(&request_id, &response)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Cancel a pending input request from an agent.
#[tauri::command]
pub fn agent_cancel_input(state: State<'_, AppState>, request_id: String) -> Result<bool, CommandError> {
    state
        .agent_comms
        .cancel_input_request(&request_id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Send a message to a specific agent (alias for agent_comms_send).
#[tauri::command]
pub fn agent_send_message(
    state: State<'_, AppState>,
    agent_id: String,
    method: String,
    params: String,
) -> Result<bool, CommandError> {
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| CommandError::InvalidInput(format!("Invalid params JSON: {}", e)))?;
    state
        .agent_comms
        .send_to_agent(&agent_id, &method, &params_json)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Disconnect an agent by its ID.
#[tauri::command]
pub fn agent_disconnect(state: State<'_, AppState>, agent_id: String) -> Result<bool, CommandError> {
    state
        .agent_comms
        .disconnect_agent(&agent_id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Get the agent comms session token (alias for agent_comms_token).
#[tauri::command]
pub fn agent_get_token(state: State<'_, AppState>) -> Result<String, CommandError> {
    Ok(state.agent_comms.get_comms_token().to_string())
}
