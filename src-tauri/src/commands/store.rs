use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Get a value from the persistent key-value store.
#[tauri::command]
pub fn store_get(state: State<'_, AppState>, key: String) -> Result<String, CommandError> {
    state
        .store
        .get::<String>(&key)
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Key '{}' not found", key)))
}

/// Set a value in the persistent key-value store.
#[tauri::command]
pub async fn store_set(state: State<'_, AppState>, key: String, value: String) -> Result<(), CommandError> {
    state.store.set(&key, &value).await.map_err(|e| CommandError::Internal(e.to_string()))
}

/// Check whether a key exists in the persistent key-value store.
#[tauri::command]
pub fn store_has(state: State<'_, AppState>, key: String) -> bool {
    state.store.has(&key)
}

/// Delete a key from the persistent key-value store.
#[tauri::command]
pub async fn store_delete(state: State<'_, AppState>, key: String) -> Result<(), CommandError> {
    state.store.delete(&key).await.map_err(|e| CommandError::Internal(e.to_string()))
}
