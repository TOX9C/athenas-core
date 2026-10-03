//! Shared scaffolding for the relay confirmation prompts (device pairing and
//! pane sharing). Both prompts follow the same lifecycle: listen for a Tauri
//! event, show an approve/deny dialog, keep the prompt open until the backend
//! command succeeds (so a transient relay error can be retried), and ignore
//! stale async completions targeting an older prompt.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::JsValue;

use crate::tauri_bridge;

/// Decode a bridge event payload (JSON object, or a JSON-encoded string) to
/// a JSON value for the prompt's field plucking.
fn decode_payload_value(payload: &JsValue) -> Result<serde_json::Value, serde_json::Error> {
    if let Some(inner) = payload.as_string() {
        serde_json::from_str(&inner)
    } else {
        serde_wasm_bindgen::from_value(payload.clone())
        .map_err(|e| <serde_json::Error as serde::de::Error>::custom(e.to_string()))
    }
}

/// Outcome of responding to a relay confirmation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PromptResponse {
    /// Command succeeded; close the prompt.
    Dismiss,
    /// Command failed; keep the prompt open with this error so the user can retry.
    Keep(String),
    /// The prompt moved on while the response was in flight; leave it alone.
    Ignore,
}

/// Decide what to do with a completed respond command: only the currently
/// displayed request is allowed to dismiss or annotate the prompt.
pub(super) fn response_action(
    current_id: Option<&str>,
    responded_id: &str,
    result: Result<(), &str>,
) -> PromptResponse {
    if current_id != Some(responded_id) {
        return PromptResponse::Ignore;
    }
    match result {
        Ok(()) => PromptResponse::Dismiss,
        Err(error) => PromptResponse::Keep(error.to_string()),
    }
}

/// Apply the result of a finished respond command to the prompt state.
/// `current_id` must be re-read from the store at completion time so a newer
/// prompt cannot be dismissed by a stale task.
pub(super) fn apply_response(
    current_id: Option<&str>,
    responded_id: &str,
    result: Result<(), &str>,
    mut response_error: Signal<Option<String>>,
    mut dismiss: impl FnMut(),
) {
    match response_action(current_id, responded_id, result) {
        PromptResponse::Dismiss => dismiss(),
        PromptResponse::Keep(message) => response_error.set(Some(message)),
        PromptResponse::Ignore => {}
    }
}

/// Listen once (for the component lifetime) for a relay request event and open
/// the prompt with the parsed target. `parse` returns `None` for payloads that
/// are missing the required id.
pub(super) fn use_request_listener<T: 'static>(
    event: &'static str,
    mut pending: Signal<Option<T>>,
    mut response_error: Signal<Option<String>>,
    parse: impl Fn(&serde_json::Value) -> Option<T> + 'static,
) {
    let parse = std::rc::Rc::new(parse);
    let unlisten: Rc<RefCell<Option<Box<dyn FnOnce()>>>> = use_hook(|| Rc::new(RefCell::new(None)));

    let unlisten_for_effect = unlisten.clone();
    use_effect(move || {
        if unlisten_for_effect.borrow().is_some() {
            return;
        }
        let parse = parse.clone();
        if let Ok(unlisten) = tauri_bridge::listen(event, move |payload: JsValue| {
            let Ok(val) = decode_payload_value(&payload) else {
                return;
            };
            if let Some(parsed) = parse(&val) {
                response_error.set(None);
                pending.set(Some(parsed));
            }
        }) {
            *unlisten_for_effect.borrow_mut() = Some(unlisten);
        }
    });

    let unlisten_for_drop = unlisten.clone();
    use_drop(move || {
        if let Some(unlisten) = unlisten_for_drop.borrow_mut().take() {
            unlisten();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{response_action, PromptResponse};

    #[test]
    fn successful_current_response_dismisses_the_prompt() {
        assert_eq!(
            response_action(Some("request-1"), "request-1", Ok(())),
            PromptResponse::Dismiss
        );
    }

    #[test]
    fn failed_current_response_keeps_the_prompt_visible() {
        assert_eq!(
            response_action(Some("request-1"), "request-1", Err("relay unavailable")),
            PromptResponse::Keep("relay unavailable".to_string())
        );
    }

    #[test]
    fn stale_response_cannot_dismiss_a_newer_prompt() {
        assert_eq!(
            response_action(Some("request-2"), "request-1", Ok(())),
            PromptResponse::Ignore
        );
    }

    #[test]
    fn response_with_no_prompt_is_ignored() {
        assert_eq!(
            response_action(None, "request-1", Ok(())),
            PromptResponse::Ignore
        );
    }
}
