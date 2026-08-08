use crate::state::AppState;
use tauri::{Manager, State};

// ── Browser commands (child webview) ─────────────────────────────────────────

fn get_normalized_url(url: &str) -> Result<String, String> {
    athena_browser::normalize_url(url).map_err(|e| e.to_string())
}

/// Child webview label for a given browser panel id.
fn child_label(id: &str) -> String {
    format!("browser-child-{}", id)
}

/// Find the main window. In Tauri 2.0, the default window label is "main".
fn main_window(state: &AppState) -> Result<tauri::Window, String> {
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    handle
        .get_window("main")
        .ok_or_else(|| "Main window not found".to_string())
}

/// Calculate default position and size for the right sidebar browser child webview.
fn sidebar_bounds(
    window: &tauri::Window,
) -> Result<
    (
        tauri_runtime::dpi::LogicalPosition<f64>,
        tauri_runtime::dpi::LogicalSize<f64>,
    ),
    String,
> {
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let sidebar_w = 420u32;
    let x = size.width.saturating_sub(sidebar_w).saturating_sub(15) as f64;
    let y = 120.0f64; // below header/toolbar
    let w = sidebar_w as f64;
    let h = (size.height.saturating_sub(120).saturating_sub(60)) as f64;
    Ok((
        tauri_runtime::dpi::LogicalPosition::new(x, y),
        tauri_runtime::dpi::LogicalSize::new(w, h),
    ))
}

/// Open (show) a browser panel — creates the child webview if not already present.
#[tauri::command]
pub fn browser_show(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    let normalized = get_normalized_url(&url)?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if handle.get_webview(&label).is_none() {
        let w = main_window(&state)?;
        let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
        let builder = tauri::WebviewBuilder::new(&label, tauri::WebviewUrl::External(parsed));
        let (pos, sz) = sidebar_bounds(&w)?;
        w.add_child(builder, pos, sz).map_err(|e| e.to_string())?;
        {
            let mut labels = state.child_webview_labels.lock();
            labels.insert(label);
        }
    }

    state
        .browser_manager
        .open_browser(&id, &normalized)
        .map_err(|e| e.to_string())
}

/// Hide (close) a browser panel — destroys the child webview.
#[tauri::command]
pub fn browser_hide(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let _ = webview.close();
    }
    {
        let mut labels = state.child_webview_labels.lock();
        labels.remove(&label);
    }

    state
        .browser_manager
        .close_browser(&id)
        .map_err(|e| e.to_string())
}

/// Navigate a browser panel to a new URL.
#[tauri::command]
pub fn browser_navigate(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    let normalized = get_normalized_url(&url)?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
    } else {
        let w = main_window(&state)?;
        let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
        let builder = tauri::WebviewBuilder::new(&label, tauri::WebviewUrl::External(parsed));
        let (pos, sz) = sidebar_bounds(&w)?;
        w.add_child(builder, pos, sz).map_err(|e| e.to_string())?;
        {
            let mut labels = state.child_webview_labels.lock();
            labels.insert(label);
        }
    }

    state
        .browser_manager
        .navigate(&id, &normalized)
        .map_err(|e| e.to_string())
}

/// Navigate back in browser history.
#[tauri::command]
pub fn browser_back(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let url = state
        .browser_manager
        .go_back(&id)
        .map_err(|e| e.to_string())?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let parsed = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
    }

    Ok(url)
}

/// Navigate forward in browser history.
#[tauri::command]
pub fn browser_forward(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let url = state
        .browser_manager
        .go_forward(&id)
        .map_err(|e| e.to_string())?;
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        let parsed = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
    }

    Ok(url)
}

/// Reload the current browser page.
#[tauri::command]
pub fn browser_reload(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        webview.reload().map_err(|e| e.to_string())?;
    }

    state.browser_manager.reload(&id).map_err(|e| e.to_string())
}

/// Reposition/resize the browser child webview to match a frontend-measured rect.
///
/// The frontend owns a placeholder `<div>` and reports its on-screen bounds (in
/// logical pixels) so the native child webview tracks the resizable sidebar, the
/// main-area panel, and window resizes. No-op if the webview doesn't exist yet.
/// Passing off-screen coordinates "parks" the webview (keeps the page alive while
/// hidden) when its surface unmounts.
#[tauri::command]
pub fn browser_set_bounds(
    state: State<'_, AppState>,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let label = child_label(&id);
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    if let Some(webview) = handle.get_webview(&label) {
        webview
            .set_position(tauri_runtime::dpi::LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        webview
            .set_size(tauri_runtime::dpi::LogicalSize::new(
                width.max(0.0),
                height.max(0.0),
            ))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
