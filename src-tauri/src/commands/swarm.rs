use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Read the current swarm state from the given directory.
#[tauri::command]
pub async fn swarm_read_state(state: State<'_, AppState>, dir: String) -> Result<String, CommandError> {
    let coordinator = state.swarm_coordinator.lock().await;
    let result = coordinator
        .read_state(&dir)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match result {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
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
) -> Result<(), CommandError> {
    let coordinator = state.swarm_coordinator.lock().await;
    coordinator
        .send_message(&dir, &from, &to, &content)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Read all messages from a swarm agent's mailbox.
#[tauri::command]
pub async fn swarm_read_mailbox(
    state: State<'_, AppState>,
    dir: String,
    agent_id: String,
) -> Result<String, CommandError> {
    let coordinator = state.swarm_coordinator.lock().await;
    let messages = coordinator
        .read_mailbox(&dir, &agent_id)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    serde_json::to_string(&messages).map_err(|e| CommandError::Internal(e.to_string()))
}
