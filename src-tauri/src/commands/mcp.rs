use super::caps;
use crate::state::AppState;
use tauri::State;

/// Ensure the Rust MCP TCP server is listening on `port`.
///
/// The desktop app starts the canonical listener on 127.0.0.1:4545 during
/// backend initialization. Re-requesting that same port is idempotent;
/// requesting a different port while the listener is active is rejected so a
/// caller cannot mistake the boot listener for an alternate bound port.
#[tauri::command]
pub async fn mcp_init(state: State<'_, AppState>, port: u16) -> Result<(), String> {
    let mut server = state.mcp_server.lock().await;
    if let Some(active_port) = server.port() {
        return if active_port == port {
            Ok(())
        } else {
            Err(format!(
                "MCP server is already listening on port {active_port}; requested {port}"
            ))
        };
    }
    server.init(port).map_err(|e| e.to_string())
}

/// Shut down the MCP server.
#[tauri::command]
pub async fn mcp_shutdown(state: State<'_, AppState>) -> Result<(), String> {
    let mut server = state.mcp_server.lock().await;
    server.request_shutdown();
    server.wait_for_tcp_shutdown().await;
    server.shutdown();
    Ok(())
}

/// Handle a JSON-RPC request through the MCP server.
#[tauri::command]
pub async fn mcp_handle_request(
    state: State<'_, AppState>,
    request: String,
) -> Result<String, String> {
    if request.len() > caps::MAX_REQUEST_BYTES {
        return Err(format!(
            "request too large: {} > {}",
            request.len(),
            caps::MAX_REQUEST_BYTES
        ));
    }
    let req =
        athena_core::mcp::McpServer::parse_request(&request).ok_or("Invalid JSON-RPC request")?;

    let server = state.mcp_server.lock().await;
    let resp = match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        server.handle_request(&req),
    )
    .await
    {
        Ok(resp) => resp,
        Err(_) => {
            log::warn!(
                "MCP handle_request timed out after 60s for method {}",
                req.method
            );
            athena_core::mcp::JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                result: None,
                error: Some(athena_core::mcp::JsonRpcError {
                    code: -32603,
                    message: "request timed out".into(),
                    data: None,
                }),
            }
        }
    };
    Ok(athena_core::mcp::McpServer::serialize_response(&resp))
}

/// Broadcast a notification to all connected MCP clients.
#[tauri::command]
pub async fn mcp_broadcast(
    state: State<'_, AppState>,
    method: String,
    params: String,
) -> Result<(), String> {
    let server = state.mcp_server.lock().await;
    let params_json: serde_json::Value =
        serde_json::from_str(&params).map_err(|e| e.to_string())?;
    server.broadcast_notification(&method, &params_json);
    Ok(())
}

/// List all tools exposed by the MCP server.
#[tauri::command]
pub fn mcp_tools() -> Result<String, String> {
    let tools = athena_core::mcp::get_tools();
    serde_json::to_string(&tools).map_err(|e| e.to_string())
}
