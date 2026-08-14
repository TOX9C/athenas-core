use crate::state::AppState;
use tauri::webview::NewWindowResponse;
use tauri::{Manager, State};

// ── Browser commands (child webview) ─────────────────────────────────────────

const MAX_BROWSER_ID_BYTES: usize = 64;
const MAX_BROWSER_COORDINATE: f64 = 100_000.0;
const MAX_BROWSER_DIMENSION: f64 = 20_000.0;

fn get_normalized_url(url: &str) -> Result<String, String> {
    athena_browser::normalize_url(url).map_err(|e| e.to_string())
}

/// Validate the logical panel id before using it in a native WebView label.
/// Labels are intentionally narrow so user-controlled input cannot introduce
/// separators, whitespace, or surprising native labels.
fn validate_browser_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_BROWSER_ID_BYTES {
        return Err("Browser id must be 1-64 bytes".to_string());
    }
    if !id.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
    }) {
        return Err("Browser id contains unsupported characters".to_string());
    }
    Ok(())
}

/// Child webview label for a given browser panel id.
fn child_label(id: &str) -> Result<String, String> {
    validate_browser_id(id)?;
    Ok(format!("browser-child-{id}"))
}

/// Find the main window. In Tauri 2.0, the default window label is "main".
fn main_window(state: &AppState) -> Result<tauri::Window, String> {
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    handle
        .get_window("main")
        .ok_or_else(|| "Main window not found".to_string())
}

/// Build and attach one native child webview, wiring its page lifecycle back
/// into the platform-independent BrowserManager.
fn create_child_webview(state: &AppState, id: &str, url: &str) -> Result<(), String> {
    let label = child_label(id)?;
    let window = main_window(state)?;
    let parsed = tauri::Url::parse(url).map_err(|e| e.to_string())?;
    let manager_for_navigation = state.browser_manager.clone();
    let manager_for_title = state.browser_manager.clone();
    let manager_for_load = state.browser_manager.clone();
    let event_id = id.to_string();
    let event_id_for_navigation = event_id.clone();
    let event_id_for_title = event_id.clone();
    let event_id_for_load = event_id.clone();

    // Keep links that request a new window inside the same browser surface.
    // Many search providers use target=_blank for result links; without this
    // handler WKWebView silently denies the request and the click appears to
    // do nothing. The initialization script below also rewrites those links
    // before the click reaches WebKit, while this handler denies any remaining
    // popup request rather than creating an unmanaged native window.
    const SAME_SURFACE_LINK_SCRIPT: &str = r#"
        (() => {
            document.addEventListener('click', (event) => {
                const link = event.target && event.target.closest
                    ? event.target.closest('a[target="_blank"]')
                    : null;
                if (link) link.target = '_self';
            }, true);
            window.open = (url) => {
                if (url) window.location.href = String(url);
                return window;
            };
        })();
    "#;

    let builder = tauri::WebviewBuilder::new(&label, tauri::WebviewUrl::External(parsed))
        .initialization_script(SAME_SURFACE_LINK_SCRIPT)
        .on_new_window(|_url, _features| NewWindowResponse::Deny)
        // The browser is deliberately an HTTP(S)-only surface. This also
        // protects redirects and clicked links, not only URL-bar commands.
        .on_navigation(move |url| {
            // Record every allowed navigation request before page-load callbacks
            // arrive. This gives the backend a URL correlation key for the
            // active generation and lets it reject delayed stale callbacks.
            athena_browser::normalize_url(url.as_str()).is_ok()
                && manager_for_navigation
                    .observe_navigation(&event_id_for_navigation, url.as_str())
                    .ok()
                    .flatten()
                    .is_some()
        })
        .on_document_title_changed(move |_webview, title| {
            let _ = manager_for_title.set_title(&event_id_for_title, &title);
        })
        .on_page_load(move |webview, payload| {
            let url = payload.url().as_str();
            let phase = match payload.event() {
                tauri::webview::PageLoadEvent::Started => athena_browser::PageLoadPhase::Started,
                tauri::webview::PageLoadEvent::Finished => athena_browser::PageLoadPhase::Finished,
            };
            // Tauri does not expose a native navigation ID. On completion,
            // cross-check the callback URL against WebKit's currently committed
            // URL so a delayed callback from an older generation cannot settle
            // the newer navigation. During Started, WebKit still reports the
            // previous committed page, so only the backend generation gate is
            // used.
            if matches!(phase, athena_browser::PageLoadPhase::Finished) {
                // If WebKit cannot report its committed URL, do not let an
                // uncorrelated callback settle the active generation. The
                // frontend timeout will surface the unresolved load instead.
                if let Ok(committed_url) = webview.url() {
                    let _ = manager_for_load.apply_page_load_for_current_url(
                        &event_id_for_load,
                        url,
                        committed_url.as_str(),
                        phase,
                    );
                }
            } else {
                let _ = manager_for_load.apply_page_load(&event_id_for_load, url, phase);
            }
        });
    // Start parked to avoid flashing at hard-coded sidebar coordinates. The
    // frontend places the child after its first measured viewport rectangle.
    let pos = tauri_runtime::dpi::LogicalPosition::new(-20_000.0, -20_000.0);
    let sz = tauri_runtime::dpi::LogicalSize::new(800.0, 600.0);
    window
        .add_child(builder, pos, sz)
        .map_err(|e| e.to_string())?;

    state.child_webview_labels.lock().insert(label);
    Ok(())
}

/// Close every browser child and clear the model. Used during app shutdown.
pub fn shutdown_browser_children(state: &AppState) {
    let _operation_guard = state.browser_manager.operation_guard().ok();
    if let Some(handle) = state.get_app_handle() {
        let labels: Vec<String> = state.child_webview_labels.lock().iter().cloned().collect();
        for label in labels {
            if let Some(webview) = handle.get_webview(&label) {
                let _ = webview.close();
            }
        }
    }
    state.child_webview_labels.lock().clear();
    state.browser_manager.shutdown();
}

/// Open (show) a browser panel — idempotently creates the child webview.
#[tauri::command]
pub fn browser_show(state: State<'_, AppState>, id: String, url: String) -> Result<String, String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    validate_browser_id(&id)?;
    let normalized = get_normalized_url(&url)?;
    let label = child_label(&id)?;
    let has_model = state
        .browser_manager
        .has_panel(&id)
        .map_err(|e| e.to_string())?;

    // A remount after docking must not replace the page with DEFAULT_URL or
    // fail with PanelAlreadyExists. The existing model is the source of truth.
    if !has_model {
        state
            .browser_manager
            .open_browser(&id, &normalized)
            .map_err(|e| e.to_string())?;
    }

    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    if handle.get_webview(&label).is_none() {
        let current_url = state
            .browser_manager
            .get_active_url(&id)
            .map_err(|e| e.to_string())?;
        if let Err(error) = create_child_webview(&state, &id, &current_url) {
            if !has_model {
                let _ = state.browser_manager.close_browser(&id);
            }
            return Err(error);
        }
    }

    let snapshot = state
        .browser_manager
        .get_snapshot(&id)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&snapshot).map_err(|e| e.to_string())
}

/// Hide (close) a browser panel — idempotently destroys the child webview.
#[tauri::command]
pub fn browser_hide(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    let label = child_label(&id)?;
    if let Some(handle) = state.get_app_handle() {
        if let Some(webview) = handle.get_webview(&label) {
            webview.close().map_err(|e| e.to_string())?;
        }
    }
    state.child_webview_labels.lock().remove(&label);
    if state
        .browser_manager
        .has_panel(&id)
        .map_err(|e| e.to_string())?
    {
        state
            .browser_manager
            .close_browser(&id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Navigate a browser panel to a new URL.
#[tauri::command]
pub fn browser_navigate(state: State<'_, AppState>, id: String, url: String) -> Result<(), String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    validate_browser_id(&id)?;
    let normalized = get_normalized_url(&url)?;
    let parsed = tauri::Url::parse(&normalized).map_err(|e| e.to_string())?;
    let label = child_label(&id)?;
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;

    let previous_panel = state
        .browser_manager
        .has_panel(&id)
        .map_err(|e| e.to_string())?
        .then(|| state.browser_manager.get_panel(&id))
        .transpose()
        .map_err(|e| e.to_string())?;
    let had_model = previous_panel.is_some();

    if !had_model {
        state
            .browser_manager
            .open_browser(&id, &normalized)
            .map_err(|e| e.to_string())?;
    } else {
        state
            .browser_manager
            .navigate(&id, &normalized)
            .map_err(|e| e.to_string())?;
    }

    let native_result = if let Some(webview) = handle.get_webview(&label) {
        webview.navigate(parsed).map_err(|e| e.to_string())
    } else {
        let current_url = state
            .browser_manager
            .get_active_url(&id)
            .map_err(|e| e.to_string())?;
        create_child_webview(&state, &id, &current_url)
    };

    if let Err(error) = native_result {
        if let Some(previous_panel) = previous_panel {
            let _ = state.browser_manager.restore_panel(previous_panel);
        } else {
            let _ = state.browser_manager.close_browser(&id);
        }
        return Err(error);
    }

    Ok(())
}

/// Navigate back in browser history.
#[tauri::command]
pub fn browser_back(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    let label = child_label(&id)?;
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    let previous_panel = state
        .browser_manager
        .get_panel(&id)
        .map_err(|e| e.to_string())?;
    let url = state
        .browser_manager
        .go_back(&id)
        .map_err(|e| e.to_string())?;

    let Some(webview) = handle.get_webview(&label) else {
        let _ = state.browser_manager.restore_panel(previous_panel);
        return Err("Browser child webview not found".to_string());
    };
    let parsed = match tauri::Url::parse(&url) {
        Ok(parsed) => parsed,
        Err(error) => {
            let _ = state.browser_manager.restore_panel(previous_panel);
            return Err(error.to_string());
        }
    };
    if let Err(error) = webview.navigate(parsed) {
        let _ = state.browser_manager.restore_panel(previous_panel);
        return Err(error.to_string());
    }

    Ok(url)
}

/// Navigate forward in browser history.
#[tauri::command]
pub fn browser_forward(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    let label = child_label(&id)?;
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    let previous_panel = state
        .browser_manager
        .get_panel(&id)
        .map_err(|e| e.to_string())?;
    let url = state
        .browser_manager
        .go_forward(&id)
        .map_err(|e| e.to_string())?;

    let Some(webview) = handle.get_webview(&label) else {
        let _ = state.browser_manager.restore_panel(previous_panel);
        return Err("Browser child webview not found".to_string());
    };
    let parsed = match tauri::Url::parse(&url) {
        Ok(parsed) => parsed,
        Err(error) => {
            let _ = state.browser_manager.restore_panel(previous_panel);
            return Err(error.to_string());
        }
    };
    if let Err(error) = webview.navigate(parsed) {
        let _ = state.browser_manager.restore_panel(previous_panel);
        return Err(error.to_string());
    }

    Ok(url)
}

/// Reload the current browser page.
#[tauri::command]
pub fn browser_reload(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    let label = child_label(&id)?;
    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    let previous_panel = state
        .browser_manager
        .get_panel(&id)
        .map_err(|e| e.to_string())?;

    // Register the new generation before WebKit can synchronously emit a
    // reload callback. If the native operation fails, restore the prior model.
    state
        .browser_manager
        .reload(&id)
        .map_err(|e| e.to_string())?;
    if let Some(webview) = handle.get_webview(&label) {
        if let Err(error) = webview.reload() {
            let _ = state.browser_manager.restore_panel(previous_panel);
            return Err(error.to_string());
        }
    }

    Ok(())
}

/// Reposition/resize the browser child webview to match a frontend-measured rect.
/// Off-screen coordinates are allowed for parking, but all values must be finite
/// and bounded so malformed IPC cannot feed invalid native geometry.
#[tauri::command]
pub fn browser_set_bounds(
    state: State<'_, AppState>,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let _operation_guard = state
        .browser_manager
        .operation_guard()
        .map_err(|e| e.to_string())?;
    let label = child_label(&id)?;
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || x.abs() > MAX_BROWSER_COORDINATE
        || y.abs() > MAX_BROWSER_COORDINATE
        || width < 0.0
        || height < 0.0
        || width > MAX_BROWSER_DIMENSION
        || height > MAX_BROWSER_DIMENSION
    {
        return Err("Browser bounds are invalid or out of range".to_string());
    }

    let handle = state.get_app_handle().ok_or("AppHandle not available")?;
    if let Some(webview) = handle.get_webview(&label) {
        webview
            .set_position(tauri_runtime::dpi::LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        webview
            .set_size(tauri_runtime::dpi::LogicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{child_label, validate_browser_id};

    #[test]
    fn browser_ids_are_safe_for_native_labels() {
        assert_eq!(
            child_label("sidebar-browser").unwrap(),
            "browser-child-sidebar-browser"
        );
        for id in ["", "Sidebar", "a/b", "a b", "a:b"] {
            assert!(
                validate_browser_id(id).is_err(),
                "expected rejection: {id:?}"
            );
        }
    }
}
