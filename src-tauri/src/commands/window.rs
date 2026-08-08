use tauri::{AppHandle, Manager};

/// Minimize the main application window.
#[tauri::command]
pub fn window_minimize(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.minimize().map_err(|error| error.to_string())
}

/// Maximize or restore the main application window.
#[tauri::command]
pub fn window_maximize(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.maximize().map_err(|error| error.to_string())
}

/// Close the main application window.
#[tauri::command]
pub fn window_close(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.close().map_err(|error| error.to_string())
}

/// Check whether the main window is currently maximized.
#[tauri::command]
pub fn window_is_maximized(app_handle: AppHandle) -> Result<bool, String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.is_maximized().map_err(|error| error.to_string())
}

/// Return the current platform identifier (e.g., `"macos"`, `"linux"`, `"windows"`).
#[tauri::command]
pub fn window_platform() -> String {
    std::env::consts::OS.to_string()
}
