use crate::components::shared::modal::Modal;
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A pending Mobile Mirror pairing request surfaced by the backend's
/// `relay:pairingRequest` event. The desktop operator must approve it before
/// the phone's WebSocket session is granted.
#[derive(Clone, PartialEq)]
struct PendingPairing {
    request_id: String,
    peer: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PromptResponse {
    Dismiss,
    Keep(String),
    Ignore,
}

fn response_action(
    pending: Option<&PendingPairing>,
    request_id: &str,
    result: Result<(), &str>,
) -> PromptResponse {
    if pending.is_none_or(|current| current.request_id != request_id) {
        return PromptResponse::Ignore;
    }
    match result {
        Ok(()) => PromptResponse::Dismiss,
        Err(error) => PromptResponse::Keep(error.to_string()),
    }
}

/// Approve/deny the pending pairing. Keep the prompt visible until the native
/// command succeeds so a transient relay error can be retried.
fn respond_to_pairing(
    pending: Signal<Option<PendingPairing>>,
    response_error: Signal<Option<String>>,
    approved: bool,
) {
    let Some(id) = pending.read().as_ref().map(|p| p.request_id.clone()) else {
        return;
    };
    let mut pending = pending;
    let mut response_error = response_error;
    response_error.set(None);
    spawn(async move {
        let result = tauri_bridge::relay_pairing_respond(&id, approved)
            .await
            .map_err(|error| format!("{error:?}"));
        let action = response_action(
            pending.read().as_ref(),
            &id,
            result.as_ref().map(|_| ()).map_err(|error| error.as_str()),
        );
        match action {
            PromptResponse::Dismiss => pending.set(None),
            PromptResponse::Keep(message) => response_error.set(Some(message)),
            PromptResponse::Ignore => {}
        }
    });
}

/// Top-level pairing-confirmation dialog. Mounted once for the app lifetime;
/// listens for `relay:pairingRequest` and shows an approve/deny prompt so the
/// desktop operator gates each new phone connection (closing the LAN
/// token-sniffing replay hole).
#[component]
pub fn RelayPairingPrompt() -> Element {
    let pending: Signal<Option<PendingPairing>> = use_signal(|| None);
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
            tauri_bridge::listen("relay:pairingRequest", move |payload: String| {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    return;
                };
                let request_id = val
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let peer = val
                    .get("peer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown device")
                    .to_string();
                if !request_id.is_empty() {
                    response_error_for_listener.set(None);
                    pending_for_listener.set(Some(PendingPairing { request_id, peer }));
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

    let Some(current) = pending.read().as_ref().cloned() else {
        return rsx! {};
    };
    let response_error_message = response_error.read().clone();

    rsx! {
        Modal {
            title: "Approve device pairing?".to_string(),
            compact: true,
            width: 460,
            on_close: move |_| respond_to_pairing(pending, response_error, false),
            footer: rsx! {
                button { class: "btn-ghost", onclick: move |_| respond_to_pairing(pending, response_error, false), "Deny" }
                button { class: "btn-primary", onclick: move |_| respond_to_pairing(pending, response_error, true), "Allow" }
            },
            div {
                style: "display: flex; flex-direction: column; gap: 10px;",
                p {
                    style: "margin: 0; color: var(--text);",
                    "A device on your local network is trying to pair with this Athena desktop through Mobile Mirror."
                }
                div {
                    style: "font-family: var(--font-mono, ui-monospace, monospace); font-size: var(--text-xs); color: var(--textMuted); word-break: break-all; background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 8px 10px;",
                    "{current.peer}"
                }
                p {
                    style: "margin: 0; font-size: var(--text-xs); color: var(--textDim);",
                    "If you didn't just scan the QR code or open the connection link, deny this request."
                }
                if let Some(message) = response_error_message {
                    p {
                        style: "margin: 0; color: var(--danger, #ef4444);",
                        "Could not respond to the pairing request: {message}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{response_action, PendingPairing, PromptResponse};

    #[test]
    fn successful_current_response_dismisses_the_prompt() {
        let pending = PendingPairing {
            request_id: "request-1".to_string(),
            peer: "phone".to_string(),
        };

        assert_eq!(
            response_action(Some(&pending), "request-1", Ok(())),
            PromptResponse::Dismiss
        );
    }

    #[test]
    fn failed_current_response_keeps_the_prompt_visible() {
        let pending = PendingPairing {
            request_id: "request-1".to_string(),
            peer: "phone".to_string(),
        };

        assert_eq!(
            response_action(Some(&pending), "request-1", Err("relay unavailable")),
            PromptResponse::Keep("relay unavailable".to_string())
        );
    }

    #[test]
    fn stale_response_cannot_dismiss_a_newer_prompt() {
        let pending = PendingPairing {
            request_id: "request-2".to_string(),
            peer: "new phone".to_string(),
        };

        assert_eq!(
            response_action(Some(&pending), "request-1", Ok(())),
            PromptResponse::Ignore
        );
    }
}
