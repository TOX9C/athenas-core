use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Execute a built-in tool by name with the given arguments.
#[tauri::command]
pub async fn tool_execute(
    state: State<'_, AppState>,
    tool_name: String,
    arguments: String,
) -> Result<String, CommandError> {
    let tool_executor = state.tool_executor.clone();
    tokio::task::spawn_blocking(move || {
        let executor = tool_executor.lock().map_err(|e| CommandError::Internal(e.to_string()))?;
        let input: athena_core::tool_executor::ToolInput =
            serde_json::from_str(&arguments).map_err(|e| CommandError::InvalidInput(format!("Invalid arguments JSON: {}", e)))?;
        let result = executor
            .execute_tool_call(&tool_name, &input)
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        serde_json::to_string(&result).map_err(|e| CommandError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| CommandError::Internal(e.to_string()))?
}

/// List all built-in tools available in the tool executor.
#[tauri::command]
pub fn tool_list() -> Result<String, CommandError> {
    let tools = athena_core::tool_executor::orchestrator_tools();
    serde_json::to_string(&tools).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Get the OpenAI-compatible tool schemas for all built-in tools.
#[tauri::command]
pub fn tool_openai_schema() -> Result<String, CommandError> {
    let schemas = athena_core::tool_executor::to_openai_tools();
    serde_json::to_string(&schemas).map_err(|e| CommandError::Internal(e.to_string()))
}
