use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Create a new execution plan with a goal, reasoning, and steps.
#[tauri::command]
pub fn plan_create(
    state: State<'_, AppState>,
    goal: String,
    reasoning: String,
    steps: String,
) -> Result<String, CommandError> {
    let step_list: Vec<athena_core::plan_manager::PlanStepInput> =
        serde_json::from_str(&steps).map_err(|e| CommandError::InvalidInput(format!("Invalid steps JSON: {}", e)))?;
    let input = athena_core::plan_manager::PlanInput {
        goal,
        reasoning,
        steps: step_list,
    };
    let plan = state
        .plan_manager
        .set_active_plan(input)
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    serde_json::to_string(&plan).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Get the currently active plan, if any.
#[tauri::command]
pub fn plan_get(state: State<'_, AppState>) -> Result<String, CommandError> {
    let plan = state.plan_manager.get_active_plan();
    serde_json::to_string(&plan).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Update the status of a specific step in the active plan.
#[tauri::command]
pub fn plan_update_step(
    state: State<'_, AppState>,
    step_id: String,
    status: String,
    pane_id: Option<String>,
) -> Result<bool, CommandError> {
    let step_status = match status.as_str() {
        "pending" => athena_core::plan_manager::StepStatus::Pending,
        "in_progress" => athena_core::plan_manager::StepStatus::InProgress,
        "completed" => athena_core::plan_manager::StepStatus::Completed,
        "failed" => athena_core::plan_manager::StepStatus::Failed,
        _ => return Err(CommandError::InvalidInput(format!("Invalid status: '{}'", status))),
    };
    state
        .plan_manager
        .update_step_status(&step_id, step_status, pane_id.as_deref())
        .map_err(|e| CommandError::Internal(e.to_string()))
}
