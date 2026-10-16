//! Where should this terminal link open? — the chooser shown instead of
//! silently navigating the in-app browser.
//!
//! Mounted once for the app lifetime at the root, so the xterm link handler
//! (which runs outside any Dioxus scope) only has to stage
//! `pending_link_request` and let this component own the layout mutation.
//!
//! Built on the shared `Modal` rather than a plain popover: macOS renders the
//! embedded browser as a native child WebView, and CSS z-index cannot cover
//! it. `Modal` parks that WebView via `use_modal_overlay_store`, so the
//! chooser actually appears in front.

use crate::components::right_sidebar::browser_surface::BROWSER_ID;
use crate::components::shared::modal::Modal;
use crate::stores::panel_manager::{use_panel_manager_store, PanelManagerState};
use crate::stores::ui::{use_ui_store, UIState};
use crate::tauri_bridge;
use crate::utils::open_link::{clear_link_request, link_display_host, open_link_in_browser};
use dioxus::prelude::*;

/// Resolve a staged terminal link into the in-app browser. Mirrors the
/// pre-chooser behavior exactly: stage the URL, bring the sidebar up on the
/// Browser tab, then navigate the shared panel to the link.
fn open_in_app_browser(ui: &mut UIState, panel: &mut PanelManagerState, url: &str) {
    open_link_in_browser(ui, panel, url);
    let url_for_navigate = url.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = tauri_bridge::browser_navigate(BROWSER_ID, &url_for_navigate).await {
            web_sys::console::error_1(
                &format!("[LinkChooser] browser_navigate failed: {error:?}").into(),
            );
        }
    });
}

/// Hand `url` to the OS default browser. A failure only logs: the chooser has
/// already closed, and the terminal link is right there for another click.
fn open_in_external_browser(url: String) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = tauri_bridge::open_external(&url).await {
            web_sys::console::error_1(
                &format!("[LinkChooser] open_external failed: {error:?}").into(),
            );
        }
    });
}

/// Link chooser for terminal link clicks.
///
/// Renders nothing unless a link click is staged. Dismissing (backdrop,
/// Escape, or the X) cancels — leaving the terminal untouched rather than
/// forcing a navigation the user never chose.
#[component]
pub fn LinkChooser() -> Element {
    let mut ui_state = use_ui_store();
    let mut panel_state = use_panel_manager_store();

    let Some(staged_url) = ui_state.read().pending_link_request.clone() else {
        return rsx! {};
    };

    // Snapshot for the dialog label; the click handlers re-read the staged URL
    // so a second click while the dialog is open cannot use a stale target.
    let shown_url = staged_url.clone();
    let display_host = link_display_host(&shown_url);

    let mut dismiss = move |_| {
        clear_link_request(&mut ui_state.write());
    };

    let fallback_url = shown_url.clone();
    let mut on_app_browser = move |_| {
        let url = ui_state
            .read()
            .pending_link_request
            .clone()
            .unwrap_or_else(|| fallback_url.clone());
        {
            let mut ui = ui_state.write();
            let mut panel = panel_state.write();
            open_in_app_browser(&mut ui, &mut panel, &url);
        }
        clear_link_request(&mut ui_state.write());
    };

    let external_fallback = shown_url.clone();
    let mut on_external = move |_| {
        let url = ui_state
            .read()
            .pending_link_request
            .clone()
            .unwrap_or_else(|| external_fallback.clone());
        open_in_external_browser(url);
        clear_link_request(&mut ui_state.write());
    };

    rsx! {
        Modal {
            title: "Open link?".to_string(),
            on_close: dismiss,
            width: 440,
            compact: true,
            footer: rsx! {
                button {
                    class: "btn-ghost",
                    onclick: move |_| dismiss(()),
                    "Cancel"
                }
                button {
                    class: "btn-primary",
                    onclick: move |_| on_app_browser(()),
                    "App Browser"
                }
                button {
                    class: "btn-secondary",
                    onclick: move |_| on_external(()),
                    "External Browser"
                }
            },
            div {
                style: "color: var(--textMuted); font-size: var(--text-sm); line-height: 1.5;",
                "You clicked a link in a terminal. Choose where to open it:"
            }
            div {
                style: "margin-top: 8px; font-family: var(--font-mono, monospace); font-size: 13px; color: var(--text); word-break: break-all;",
                "{display_host}"
            }
        }
    }
}
