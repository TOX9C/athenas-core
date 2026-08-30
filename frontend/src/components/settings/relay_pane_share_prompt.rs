use crate::components::shared::modal::Modal;
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PromptResponse {
    Dismiss,
    Keep(String),
    Ignore,
}

fn response_action(
    pending: Option<&str>,
    pane_id: &str,
    result: Result<(), &str>,
) -> PromptResponse {
    if pending != Some(pane_id) {
        return PromptResponse::Ignore;
    }
    match result {
        Ok(()) => PromptResponse::Dismiss,
        Err(error) => PromptResponse::Keep(error.to_string()),
    }
}

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
        let action = response_action(
            pending.read().as_deref(),
            &pane_id,
            result.as_ref().map(|_| ()).map_err(|error| error.as_str()),
        );
        match action {
            PromptResponse::Dismiss => pending.set(None),
            PromptResponse::Keep(message) => response_error.set(Some(message)),
            PromptResponse::Ignore => {}
        }
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
    let unlisten: Rc<RefCell<Option<Box<dyn FnOnce()>>>> = use_hook(|| Rc::new(RefCell::new(None)));

    let unlisten_for_effect = unlisten.clone();
    let mut pending_for_listener = pending;
    let mut response_error_for_listener = response_error;
    use_effect(move || {
        if unlisten_for_effect.borrow().is_some() {
            return;
        }

        if let Ok(unlisten) =
            tauri_bridge::listen("relay:paneShareRequest", move |payload: String| {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    return;
                };
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !pane_id.is_empty() {
                    response_error_for_listener.set(None);
                    pending_for_listener.set(Some(pane_id));
                }
            })
        {
            *unlisten_for_effect.borrow_mut() = Some(unlisten);
        }
    });

    let unlisten_for_drop = unlisten.clone();
    use_drop(move || {
        if let Some(unlisten) = unlisten_for_drop.borrow_mut().take() {
            unlisten();
        }
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

#[cfg(test)]
mod tests {
    use super::{response_action, PromptResponse};

    #[test]
    fn successful_current_share_response_dismisses_the_prompt() {
        assert_eq!(
            response_action(Some("pane-1"), "pane-1", Ok(())),
            PromptResponse::Dismiss
        );
    }

    #[test]
    fn failed_share_response_keeps_the_prompt_visible() {
        assert_eq!(
            response_action(Some("pane-1"), "pane-1", Err("relay unavailable")),
            PromptResponse::Keep("relay unavailable".to_string())
        );
    }

    #[test]
    fn stale_share_response_cannot_dismiss_a_newer_prompt() {
        assert_eq!(
            response_action(Some("pane-2"), "pane-1", Ok(())),
            PromptResponse::Ignore
        );
    }
}
