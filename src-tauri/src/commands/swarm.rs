use super::validate_path_exists;
use crate::state::AppState;
use athena_core::swarm::SwarmState;
use tauri::State;

fn validate_dir(state: &AppState, dir: &str) -> Result<(), String> {
    validate_path_exists(&state.store, std::path::Path::new(dir))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn serialize_state(state: SwarmState) -> Result<String, String> {
    serde_json::to_string(&state).map_err(|e| e.to_string())
}

/// Create and persist a complete swarm mission, then start its watcher.
#[tauri::command]
pub async fn swarm_create(
    state: State<'_, AppState>,
    dir: String,
    swarm_state: String,
) -> Result<String, String> {
    validate_dir(&state, &dir)?;
    let parsed: SwarmState = serde_json::from_str(&swarm_state).map_err(|e| e.to_string())?;
    let coordinator = state.swarm_coordinator.lock().await;
    serialize_state(
        coordinator
            .create_swarm(&dir, parsed)
            .await
            .map_err(|e| e.to_string())?,
    )
}

/// Read the current swarm state. `null` means the workspace has no mission.
#[tauri::command]
pub async fn swarm_read_state(state: State<'_, AppState>, dir: String) -> Result<String, String> {
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    match coordinator
        .read_state(&dir)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(s) => serialize_state(s),
        None => Ok("null".to_string()),
    }
}

/// Start monitoring a workspace's persisted swarm state.
#[tauri::command]
pub async fn swarm_start_watch(state: State<'_, AppState>, dir: String) -> Result<(), String> {
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    coordinator
        .watch_state(&dir)
        .await
        .map_err(|e| e.to_string())
}

/// Stop monitoring a workspace's persisted swarm state.
#[tauri::command]
pub fn swarm_stop_watch(state: State<'_, AppState>, dir: String) -> Result<(), String> {
    validate_dir(&state, &dir)?;
    let coordinator = state
        .swarm_coordinator
        .try_lock()
        .map_err(|_| "Swarm coordinator is busy".to_string())?;
    coordinator.stop_watch(&dir).map_err(|e| e.to_string())
}

/// Update an agent's live status and activity metadata.
#[tauri::command]
pub async fn swarm_update_agent(
    state: State<'_, AppState>,
    dir: String,
    agent_id: String,
    status: Option<String>,
    last_action: Option<String>,
    current_task: Option<Option<String>>,
) -> Result<String, String> {
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    serialize_state(
        coordinator
            .update_agent(&dir, &agent_id, status, last_action, current_task)
            .await
            .map_err(|e| e.to_string())?,
    )
}

/// Set the overall mission status: active, paused, completed, or cancelled.
#[tauri::command]
pub async fn swarm_set_status(
    state: State<'_, AppState>,
    dir: String,
    status: String,
) -> Result<String, String> {
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    serialize_state(
        coordinator
            .set_status(&dir, &status)
            .await
            .map_err(|e| e.to_string())?,
    )
}

/// Create a queued task assigned to a known swarm agent.
#[tauri::command]
pub async fn swarm_create_task(
    state: State<'_, AppState>,
    dir: String,
    title: String,
    description: String,
    assigned_agent_id: String,
) -> Result<String, String> {
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    serialize_state(
        coordinator
            .create_task(&dir, title, description, assigned_agent_id)
            .await
            .map_err(|e| e.to_string())?,
    )
}

/// Update a task status.
#[tauri::command]
pub async fn swarm_update_task(
    state: State<'_, AppState>,
    dir: String,
    task_id: String,
    status: String,
) -> Result<String, String> {
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    serialize_state(
        coordinator
            .update_task(&dir, &task_id, &status)
            .await
            .map_err(|e| e.to_string())?,
    )
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
    validate_dir(&state, &dir)?;
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
    validate_dir(&state, &dir)?;
    let coordinator = state.swarm_coordinator.lock().await;
    let messages = coordinator
        .read_mailbox(&dir, &agent_id)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&messages).map_err(|e| e.to_string())
}
