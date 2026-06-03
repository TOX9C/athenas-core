use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// List all registered plugins.
#[tauri::command]
pub fn plugin_list(state: State<'_, AppState>) -> Result<String, CommandError> {
    let plugins = state.plugin_manager.list_plugins();
    serde_json::to_string(&plugins).map_err(|e| CommandError::Internal(e.to_string()))
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
) -> Result<String, CommandError> {
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
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    Ok(id)
}

/// Unregister a plugin by its ID.
#[tauri::command]
pub fn plugin_unregister(state: State<'_, AppState>, plugin_id: String) -> Result<(), CommandError> {
    state
        .plugin_manager
        .unregister_plugin(&plugin_id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Enable a plugin by its ID.
#[tauri::command]
pub fn plugin_enable(state: State<'_, AppState>, plugin_id: String) -> Result<(), CommandError> {
    state
        .plugin_manager
        .enable_plugin(&plugin_id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Disable a plugin by its ID.
#[tauri::command]
pub fn plugin_disable(state: State<'_, AppState>, plugin_id: String) -> Result<(), CommandError> {
    state
        .plugin_manager
        .disable_plugin(&plugin_id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Get the configuration for a specific plugin.
#[tauri::command]
pub fn plugin_get_config(state: State<'_, AppState>, plugin_id: String) -> Result<String, CommandError> {
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
) -> Result<(), CommandError> {
    let config_value: serde_json::Value =
        serde_json::from_str(&config).map_err(|e| CommandError::InvalidInput(format!("Invalid config JSON: {}", e)))?;
    state
        .plugin_manager
        .set_plugin_config(&plugin_id, &config_value)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Record an error for a specific plugin.
#[tauri::command]
pub fn plugin_set_error(
    state: State<'_, AppState>,
    plugin_id: String,
    error: String,
) -> Result<(), CommandError> {
    state
        .plugin_manager
        .set_plugin_error(&plugin_id, &error)
        .map_err(|e| CommandError::Internal(e.to_string()))
}
