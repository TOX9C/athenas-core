use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Initialize the MCP server on the given port.
#[tauri::command]
pub async fn mcp_init(state: State<'_, AppState>, port: u16) -> Result<(), CommandError> {
    let mut server = state.mcp_server.lock().await;
    server.init(port).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Shut down the MCP server.
#[tauri::command]
pub async fn mcp_shutdown(state: State<'_, AppState>) -> Result<(), CommandError> {
    let mut server = state.mcp_server.lock().await;
    server.shutdown();
    Ok(())
}

/// Handle a JSON-RPC request through the MCP server.
#[tauri::command]
pub async fn mcp_handle_request(
    state: State<'_, AppState>,
    request: String,
) -> Result<String, CommandError> {
    let server = state.mcp_server.lock().await;
    let req =
        athena_core::mcp::McpServer::parse_request(&request).ok_or_else(|| {
            CommandError::InvalidInput("Invalid JSON-RPC request".to_string())
        })?;
    let resp = server.handle_request(&req).await;
    Ok(athena_core::mcp::McpServer::serialize_response(&resp))
}

/// Broadcast a notification to all connected MCP clients.
#[tauri::command]
pub async fn mcp_broadcast(
    state: State<'_, AppState>,
    method: String,
    params: String,
) -> Result<(), CommandError> {
    let server = state.mcp_server.lock().await;
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| CommandError::InvalidInput(format!("Invalid JSON params: {}", e)))?;
    server.broadcast_notification(&method, &params_json);
    Ok(())
}

/// List all tools exposed by the MCP server.
#[tauri::command]
pub fn mcp_tools() -> Result<String, CommandError> {
    let tools = athena_core::mcp::get_tools();
    serde_json::to_string(&tools).map_err(|e| CommandError::Internal(e.to_string()))
}
