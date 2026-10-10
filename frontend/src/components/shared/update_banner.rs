//! Dismissible update banner. Rendered at the top of the center column while
//! `UIState.pending_update` is `Some` — i.e. a newer GitHub release exists and
//! the user hasn't dismissed the banner for that specific version (dismissal
//! persists per-version, so a new release re-shows it).

use crate::stores::ui::use_ui_store;
use dioxus::prelude::*;

/// Key under which the dismissed version is persisted (KV store), mirroring
/// the resume-banner dismissal idiom in `terminal_grid.rs`.
const DISMISSED_KEY: &str = "update_dismissed_version";

#[component]
pub fn UpdateBanner() -> Element {
    let mut ui = use_ui_store();
    let latest = match ui.read().pending_update.clone() {
        Some(v) => v,
        None => return rsx! {},
    };
    let latest_for_dismiss = latest.clone();
    let dismiss = move |_| {
        let latest = latest_for_dismiss.clone();
        ui.write().pending_update = None;
        // Persist the dismissal (async fire-and-forget — same idiom as the
        // resume banner) so it stays hidden across app restarts.
        spawn(async move {
            let _ = crate::tauri_bridge::store_set(DISMISSED_KEY, &latest).await;
        });
    };

    rsx! {
        div {
            style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); font-size: var(--text-sm); color: var(--textDim);",
            span {
                style: "color: var(--accent); font-weight: 600;",
                "Update available"
            }
            span {
                "{latest} is out — you're on v{env!(\"CARGO_PKG_VERSION\")}."
            }
            span { style: "flex: 1;" }
            button {
                class: "btn-primary btn-sm",
                onclick: move |_| {
                    spawn(async move {
                        if let Err(e) = crate::tauri_bridge::update_open_release().await {
                            web_sys::console::warn_1(
                                &format!("[update] failed to open release page: {e:?}").into(),
                            );
                        }
                    });
                },
                "Download"
            }
            button {
                class: "btn-ghost btn-sm",
                onclick: dismiss,
                "Dismiss"
            }
        }
    }
}
