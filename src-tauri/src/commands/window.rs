use super::CommandError;
use tauri::{AppHandle, Manager};

/// Minimize the main application window.
#[tauri::command]
pub fn window_minimize(app_handle: AppHandle) -> Result<(), CommandError> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or(CommandError::NotFound("Main window not found".to_string()))?;
    window.minimize().map_err(|e| CommandError::Internal(e.to_string()))
}

/// Maximize or restore the main application window.
#[tauri::command]
pub fn window_maximize(app_handle: AppHandle) -> Result<(), CommandError> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or(CommandError::NotFound("Main window not found".to_string()))?;
    window.maximize().map_err(|e| CommandError::Internal(e.to_string()))
}

/// Close the main application window.
#[tauri::command]
pub fn window_close(app_handle: AppHandle) -> Result<(), CommandError> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or(CommandError::NotFound("Main window not found".to_string()))?;
    window.close().map_err(|e| CommandError::Internal(e.to_string()))
}

/// Check whether the main window is currently maximized.
#[tauri::command]
pub fn window_is_maximized(app_handle: AppHandle) -> Result<bool, CommandError> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or(CommandError::NotFound("Main window not found".to_string()))?;
    window.is_maximized().map_err(|e| CommandError::Internal(e.to_string()))
}

/// Return the current platform identifier (e.g., `"macos"`, `"linux"`, `"windows"`).
#[tauri::command]
pub fn window_platform() -> String {
    std::env::consts::OS.to_string()
}

/// Return the default shell path for the current platform.
#[tauri::command]
pub fn pty_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "powershell.exe".to_string()
        } else {
            "/bin/zsh".to_string()
        }
    })
}
