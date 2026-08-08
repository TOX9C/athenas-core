use super::{validate_path_exists, CommandError};
use crate::state::AppState;
use tauri::State;

// ── Plugin commands ──────────────────────────────────────────────────────────

/// List all registered plugins.
#[tauri::command]
pub fn plugin_list(state: State<'_, AppState>) -> Result<String, String> {
    let plugins = state.plugin_manager.list_plugins();
    serde_json::to_string(&plugins).map_err(|e| e.to_string())
}

/// Get detailed information about a specific plugin.
#[tauri::command]
pub fn plugin_get(state: State<'_, AppState>, plugin_id: String) -> Result<String, CommandError> {
    let plugin = state
        .plugin_manager
        .get_plugin_info(&plugin_id)
        .ok_or_else(|| CommandError::NotFound(format!("Plugin '{}' not found", plugin_id)))?;
    serde_json::to_string(&plugin).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Register a new plugin with the plugin manager.
#[tauri::command]
pub fn plugin_register(
    state: State<'_, AppState>,
    plugin_id: String,
    name: String,
    version: String,
) -> Result<String, String> {
    let manifest = athena_plugins::PluginManifest {
        id: plugin_id,
        name,
        version,
        description: String::new(),
        author: String::new(),
        permissions: vec![],
        mcp_config: None,
        min_athena_version: None,
        capabilities: vec![],
        tools: vec![],
        subscribes_to: None,
        config: None,
        install: None,
    };
    let id = state
        .plugin_manager
        .register_plugin(manifest)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Unregister a plugin by its ID.
#[tauri::command]
pub fn plugin_unregister(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Enable a plugin by its ID.
#[tauri::command]
pub fn plugin_enable(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .enable_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Disable a plugin by its ID.
#[tauri::command]
pub fn plugin_disable(state: State<'_, AppState>, plugin_id: String) -> Result<(), String> {
    state
        .plugin_manager
        .disable_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}

/// Get the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_get_config(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<String, CommandError> {
    let config = state
        .plugin_manager
        .get_plugin_config(&plugin_id)
        .ok_or_else(|| CommandError::NotFound(format!("Plugin '{}' not found", plugin_id)))?;
    serde_json::to_string(&config).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Set the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_set_config(
    state: State<'_, AppState>,
    plugin_id: String,
    config: String,
) -> Result<(), String> {
    let config_value: serde_json::Value =
        serde_json::from_str(&config).map_err(|e| e.to_string())?;
    state
        .plugin_manager
        .set_plugin_config(&plugin_id, &config_value)
        .map_err(|e| e.to_string())
}

/// Record an error for a specific plugin.
#[tauri::command]
pub fn plugin_set_error(
    state: State<'_, AppState>,
    plugin_id: String,
    error: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .set_plugin_error(&plugin_id, &error)
        .map_err(|e| e.to_string())
}

// ── Plugin host commands ─────────────────────────────────────────────────────

/// List all active plugin host sessions.
#[tauri::command]
pub fn plugin_host_list_sessions(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state.plugin_manager.list_sessions();
    serde_json::to_string(&sessions).map_err(|e| e.to_string())
}

/// Get details about a specific plugin host session.
#[tauri::command]
pub fn plugin_host_get_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, CommandError> {
    let session = state
        .plugin_manager
        .get_session(&session_id)
        .ok_or_else(|| CommandError::NotFound(format!("Session '{}' not found", session_id)))?;
    serde_json::to_string(&session).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Emit a plugin event with the given type and data.
#[tauri::command]
pub fn plugin_host_emit_event(
    state: State<'_, AppState>,
    event_type: String,
    data: String,
) -> Result<String, String> {
    let parsed_type: athena_plugins::PluginEventType =
        serde_json::from_str(&format!("\"{}\"", event_type.to_lowercase()))
            .map_err(|e| format!("Invalid event type '{}': {}", event_type, e))?;
    let payload: athena_plugins::PluginEventPayload =
        serde_json::from_str(&data).map_err(|e| format!("Invalid event payload: {}", e))?;
    let source = athena_plugins::PluginEventSource {
        session_id: String::new(),
        pane_id: None,
        agent_type: String::new(),
        agent_id: None,
    };
    let event = state
        .plugin_manager
        .emit_plugin_event(parsed_type, source, payload);
    serde_json::to_string(&event).map_err(|e| e.to_string())
}

/// Subscribe a session to specific plugin event types.
#[tauri::command]
pub fn plugin_host_subscribe(
    state: State<'_, AppState>,
    session_id: String,
    event_types: String,
) -> Result<(), String> {
    let types: Vec<athena_plugins::PluginEventType> =
        serde_json::from_str(&event_types).map_err(|e| format!("Invalid event types: {}", e))?;
    state
        .plugin_manager
        .subscribe_session(&session_id, &types)
        .map_err(|e| e.to_string())
}

/// Update the status of a plugin host session.
#[tauri::command]
pub fn plugin_host_update_status(
    state: State<'_, AppState>,
    session_id: String,
    status: String,
) -> Result<(), String> {
    let parsed_status: athena_plugins::SessionStatus =
        serde_json::from_str(&format!("\"{}\"", status.to_lowercase()))
            .map_err(|e| format!("Invalid session status '{}': {}", status, e))?;
    state
        .plugin_manager
        .update_session_status(&session_id, parsed_status, None)
        .map_err(|e| e.to_string())
}

/// Unregister a plugin host session.
#[tauri::command]
pub fn plugin_host_unregister_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .remove_session(&session_id)
        .map_err(|e| e.to_string())
}

/// Discover plugins in the given directory by scanning for manifest files.
#[tauri::command]
pub fn plugin_host_discover_plugins(
    state: State<'_, AppState>,
    dir: String,
) -> Result<String, String> {
    // Validate the plugin directory is within the workspace before scanning.
    let _ = validate_path_exists(&state.store, std::path::Path::new(&dir))
        .map_err(|e| e.to_string())?;
    let results = state
        .plugin_manager
        .discover_plugins(std::path::Path::new(&dir))
        .map_err(|e| e.to_string())?;
    // Convert inner errors to strings since PluginError doesn't implement Serialize
    let serializable: Vec<serde_json::Value> = results
        .into_iter()
        .map(|r| match r {
            Ok(manifest) => serde_json::to_value(manifest)
                .unwrap_or_else(|_| serde_json::json!({"error": "serialization failed"})),
            Err(e) => serde_json::json!({"error": e.to_string()}),
        })
        .collect();
    serde_json::to_string(&serializable).map_err(|e| e.to_string())
}

/// Register and set up a plugin with the given manifest information.
#[tauri::command]
pub fn plugin_host_setup_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    name: String,
    version: String,
) -> Result<String, String> {
    let manifest = athena_plugins::PluginManifest {
        id: plugin_id,
        name,
        version,
        description: String::new(),
        author: String::new(),
        permissions: vec![],
        mcp_config: None,
        min_athena_version: None,
        capabilities: vec![],
        tools: vec![],
        subscribes_to: None,
        config: None,
        install: None,
    };
    let id = state
        .plugin_manager
        .register_plugin(manifest)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Remove a plugin by its ID (alias for plugin_unregister).
#[tauri::command]
pub fn plugin_host_remove_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| e.to_string())
}
