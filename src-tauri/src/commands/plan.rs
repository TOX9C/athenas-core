use crate::state::AppState;
use tauri::State;

/// Get the currently active plan, if any.
#[tauri::command]
pub fn plan_get(state: State<'_, AppState>) -> Result<String, String> {
    let plan = state.plan_manager.get_active_plan();
    serde_json::to_string(&plan).map_err(|e| e.to_string())
}
