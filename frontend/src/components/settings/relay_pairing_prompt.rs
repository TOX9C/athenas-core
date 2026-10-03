use super::relay_prompt::{apply_response, use_request_listener};
use crate::components::shared::modal::Modal;
use crate::tauri_bridge;
use dioxus::prelude::*;

/// A pending Mobile Mirror pairing request surfaced by the backend's
/// `relay:pairingRequest` event. The desktop operator must approve it before
/// the phone's WebSocket session is granted.
#[derive(Clone, PartialEq)]
struct PendingPairing {
    request_id: String,
    peer: String,
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
        let current_id = pending.read().as_ref().map(|p| p.request_id.clone());
        apply_response(
            current_id.as_deref(),
            &id,
            result.as_ref().map(|_| ()).map_err(|e| e.as_str()),
            response_error,
            move || pending.set(None),
        );
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

    use_request_listener("relay:pairingRequest", pending, response_error, |val| {
        let request_id = val.get("requestId").and_then(|v| v.as_str()).unwrap_or("");
        if request_id.is_empty() {
            return None;
        }
        Some(PendingPairing {
            request_id: request_id.to_string(),
            peer: val
                .get("peer")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown device")
                .to_string(),
        })
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
