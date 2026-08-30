use crate::components::shared::icon::IconSend;
use crate::components::shared::modal::Modal;
use crate::stores::notification::{use_notification_store, NotificationType};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
/// Unique newtype so this overlay never collides with other `Signal<bool>`
/// contexts — Dioxus contexts are keyed by type, and the notification
/// popover overlay also wraps `Signal<bool>`.
#[derive(Clone, Copy)]
pub struct InputRequestOverlayState(pub Signal<bool>);

/// Shared visibility state for the actionable input-request surface.
pub fn provide_input_request_overlay_store() {
    use_context_provider(|| InputRequestOverlayState(Signal::new(true)));
}

pub fn use_input_request_overlay_store() -> Signal<bool> {
    use_context::<InputRequestOverlayState>().0
}

fn resolve_local(
    notifications: &mut Signal<Vec<crate::stores::notification::NotificationRecord>>,
    id: &str,
) {
    if let Some(record) = notifications
        .write()
        .iter_mut()
        .find(|record| record.id == id)
    {
        record.read = true;
        record.resolved_at = Some(chrono::Utc::now().timestamp_millis());
    }
}

/// Blocking response surface for backend-owned agent input requests.
///
/// Closing the modal intentionally does not mark the request read or resolved:
/// the agent is still blocked and the notification remains actionable from the
/// bell/panel. A successful response resolves the backend record by ID.
#[component]
pub fn InputRequestModal() -> Element {
    let mut free_text = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut input_open = use_input_request_overlay_store();
    let notifications = use_notification_store();

    let pending = notifications
        .read()
        .iter()
        .find(|record| {
            matches!(record.r#type, NotificationType::NeedsInput)
                && record.requires_action
                && record.resolved_at.is_none()
                && record.request_id.is_some()
        })
        .filter(|_| input_open())
        .cloned();

    let Some(pending) = pending else {
        return rsx! {};
    };

    let notification_id = pending.id.clone();
    let request_id = pending.request_id.clone().unwrap_or_default();
    let prompt_text = pending.message.clone();

    let submit = Rc::new(RefCell::new(move |response: String| {
        let response = response.trim().to_string();
        if response.is_empty() || submitting() {
            return;
        }
        submitting.set(true);
        let request_id = request_id.clone();
        let notification_id = notification_id.clone();
        let mut notifications = notifications;
        spawn(async move {
            if crate::tauri_bridge::agent_respond_input(&request_id, &response)
                .await
                .is_ok()
            {
                let _ = crate::tauri_bridge::notification_resolve(&notification_id).await;
                resolve_local(&mut notifications, &notification_id);
            }
            submitting.set(false);
        });
    }));
    let submit_enter = submit.clone();
    let submit_click = submit.clone();

    rsx! {
        Modal {
            title: "Agent Request",
            on_close: move |_| input_open.set(false),
            width: 440,

            div {
                style: "display: flex; flex-direction: column; gap: 12px;",
                div {
                    style: "display: flex; align-items: center; gap: 8px;",
                    span {
                        style: "font-size: var(--text-sm); font-weight: 600; color: var(--text); font-family: var(--font-ui);",
                        "Agent"
                    }
                    span {
                        class: "status-label",
                        style: "color: var(--warning);",
                        "Needs Input"
                    }
                }
                div {
                    style: "padding: 12px; border-radius: var(--radius-md); background: var(--bgSecondary); border: 1px solid var(--border);",
                    p {
                        style: "font-size: var(--text-sm); color: var(--text); margin: 0; line-height: 1.5;",
                        "{prompt_text}"
                    }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    input {
                        class: "field",
                        style: "flex: 1;",
                        value: "{free_text}",
                        oninput: move |event| free_text.set(event.value()),
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                event.prevent_default();
                                let response = free_text();
                                free_text.set(String::new());
                                (submit_enter.borrow_mut())(response);
                            }
                        },
                        placeholder: "Type a response..."
                    }
                    button {
                        class: "btn-primary",
                        style: "display: flex; align-items: center; gap: 6px;",
                        disabled: submitting(),
                        onclick: move |_| {
                            let response = free_text();
                            free_text.set(String::new());
                            (submit_click.borrow_mut())(response);
                        },
                        IconSend { size: Some(14), color: Some("currentColor".to_string()) }
                        if submitting() { "Sending..." } else { "Send" }
                    }
                }
            }
        }
    }
}
