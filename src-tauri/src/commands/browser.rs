use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Open a browser window with the given URL.
#[tauri::command]
pub fn browser_show(state: State<'_, AppState>, id: String, url: String) -> Result<(), CommandError> {
    state
        .browser_manager
        .open_browser(id, &url)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Close a browser window by its ID.
#[tauri::command]
pub fn browser_hide(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state
        .browser_manager
        .close_browser(&id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Navigate a browser window to a new URL.
#[tauri::command]
pub fn browser_navigate(state: State<'_, AppState>, id: String, url: String) -> Result<(), CommandError> {
    state
        .browser_manager
        .navigate(&id, &url)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Navigate a browser window back one page.
#[tauri::command]
pub fn browser_back(state: State<'_, AppState>, id: String) -> Result<String, CommandError> {
    state
        .browser_manager
        .go_back(&id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Navigate a browser window forward one page.
#[tauri::command]
pub fn browser_forward(state: State<'_, AppState>, id: String) -> Result<String, CommandError> {
    state
        .browser_manager
        .go_forward(&id)
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Reload a browser window.
#[tauri::command]
pub fn browser_reload(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.browser_manager.reload(&id).map_err(|e| CommandError::Internal(e.to_string()))
}
