use crate::state::AppState;
use tauri::State;

/// Get all kanban tasks for the active workspace, returned as a JSON array.
#[tauri::command]
pub async fn kanban_get_tasks(state: State<'_, AppState>) -> Result<String, String> {
    let workspace_id = state
        .kanban_backend
        .get_active_workspace_id()
        .map_err(|e| e.to_string())?;
    let tasks = state
        .kanban_backend
        .get_tasks(&workspace_id)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&tasks).map_err(|e| e.to_string())
}

/// Create a new kanban task in the active workspace. The task gets a generated
/// UUID, the current timestamp, and the default `Todo` status.
#[tauri::command]
pub async fn kanban_create_task(
    state: State<'_, AppState>,
    title: String,
    description: Option<String>,
) -> Result<String, String> {
    let workspace_id = state
        .kanban_backend
        .get_active_workspace_id()
        .map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let task = athena_core::kanban::KanbanBackendTask {
        id: format!("task-{}", uuid::Uuid::new_v4()),
        space_id: workspace_id.clone(),
        title,
        description,
        assigned_agent: None,
        status: athena_core::kanban::KanbanBackendStatus::Todo,
        order: 0,
        created_at: now,
    };
    let created = state
        .kanban_backend
        .create_task(&workspace_id, task)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&created).map_err(|e| e.to_string())
}

/// Update an existing kanban task in the active workspace. Only the supplied
/// fields are modified; `None` leaves them untouched.
#[tauri::command]
pub async fn kanban_update_task(
    state: State<'_, AppState>,
    task_id: String,
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
) -> Result<String, String> {
    let workspace_id = state
        .kanban_backend
        .get_active_workspace_id()
        .map_err(|e| e.to_string())?;

    let status_enum = match status {
        Some(s) => Some(
            athena_core::kanban::KanbanBackendStatus::parse(&s)
                .map_err(|e: athena_core::kanban::KanbanError| e.to_string())?,
        ),
        None => None,
    };

    let updated = state
        .kanban_backend
        .update_task(&workspace_id, &task_id, title, description, status_enum)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

/// Delete a kanban task by ID from the active workspace.
#[tauri::command]
pub async fn kanban_delete_task(state: State<'_, AppState>, task_id: String) -> Result<(), String> {
    let workspace_id = state
        .kanban_backend
        .get_active_workspace_id()
        .map_err(|e| e.to_string())?;
    state
        .kanban_backend
        .delete_task(&workspace_id, &task_id)
        .map_err(|e| e.to_string())?;
    Ok(())
}
