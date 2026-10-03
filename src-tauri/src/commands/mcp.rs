/// List all tools exposed by the MCP server.
#[tauri::command]
pub fn mcp_tools() -> Result<String, String> {
    let tools = athena_core::mcp::get_tools();
    serde_json::to_string(&tools).map_err(|e| e.to_string())
}
