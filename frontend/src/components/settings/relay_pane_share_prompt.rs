use super::relay_prompt::{apply_response, use_request_listener};
use crate::components::shared::modal::Modal;
use crate::tauri_bridge;
use dioxus::prelude::*;

/// Handle a pane-share request: approve (share the pane) or ignore (no-op).
/// Keep the prompt visible until an approval command succeeds so a transient
/// relay error can be retried. Ignore is local and can dismiss immediately.
fn respond_to_share_request(
    pending: Signal<Option<String>>,
    response_error: Signal<Option<String>>,
    approved: bool,
) {
    let Some(pane_id) = pending.read().clone() else {
        return;
    };
    if !approved {
        let mut pending = pending;
        pending.set(None);
        return;
    }

    let mut pending = pending;
    let mut response_error = response_error;
    response_error.set(None);
    spawn(async move {
        let result = tauri_bridge::relay_set_pane_shared(&pane_id, true)
            .await
            .map_err(|error| format!("{error:?}"));
        let current_id = pending.read().clone();
        apply_response(
            current_id.as_deref(),
            &pane_id,
            result.as_ref().map(|_| ()).map_err(|e| e.as_str()),
            response_error,
            move || pending.set(None),
        );
    });
}

/// Top-level pane-share prompt. Mounted once for the app lifetime; listens for
/// `relay:paneShareRequest` (a paired phone asking to access one of this
/// desktop's panes) and shows an approve/ignore dialog. Approval flips the
/// pane's share toggle via `relay_set_pane_shared`.
#[component]
pub fn RelayPaneSharePrompt() -> Element {
    let pending: Signal<Option<String>> = use_signal(|| None);
    let response_error: Signal<Option<String>> = use_signal(|| None);

    use_request_listener("relay:paneShareRequest", pending, response_error, |val| {
        let pane_id = val.get("paneId").and_then(|v| v.as_str()).unwrap_or("");
        if pane_id.is_empty() {
            return None;
        }
        Some(pane_id.to_string())
    });

    let Some(pane_id) = pending.read().clone() else {
        return rsx! {};
    };
    let response_error_message = response_error.read().clone();

    rsx! {
        Modal {
            title: "Share a pane with your phone?".to_string(),
            compact: true,
            width: 460,
            on_close: move |_| respond_to_share_request(pending, response_error, false),
            footer: rsx! {
                button { class: "btn-ghost", onclick: move |_| respond_to_share_request(pending, response_error, false), "Ignore" }
                button { class: "btn-primary", onclick: move |_| respond_to_share_request(pending, response_error, true), "Share pane" }
            },
            div {
                style: "display: flex; flex-direction: column; gap: 10px;",
                p {
                    style: "margin: 0; color: var(--text);",
                    "Your phone is asking to access a terminal pane on this desktop through Mobile Mirror."
                }
                div {
                    style: "font-family: var(--font-mono, ui-monospace, monospace); font-size: var(--text-xs); color: var(--textMuted); word-break: break-all; background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 8px 10px;",
                    "{pane_id}"
                }
                p {
                    style: "margin: 0; font-size: var(--text-xs); color: var(--textDim);",
                    "Sharing grants read and write access to that pane only. You can revoke it anytime from the pane's share toggle."
                }
                if let Some(message) = response_error_message {
                    p {
                        style: "margin: 0; color: var(--danger, #ef4444);",
                        "Could not share the pane: {message}"
                    }
                }
            }
        }
    }
}
