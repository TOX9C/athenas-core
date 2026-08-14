use crate::components::shared::icon::IconSend;
use crate::components::shared::modal::Modal;
use crate::stores::athena::use_athena_store;
use crate::stores::notification::{use_notification_store, NotificationType};
use dioxus::prelude::*;

/// Mark the first pending input-request notification as read so the modal
/// dismisses. Used by all three dismiss paths (Enter, Send button, modal
/// on_close) so they agree — previously only the Enter handler did this,
/// leaving the Send button and backdrop click re-popping the modal.
///
/// Re-derives the pending notification from the store rather than taking it
/// as an argument, so each event handler can call it independently (the
/// `pending` value computed in the component body is moved into the rsx
/// closure and can't be borrowed from multiple handlers).
fn dismiss_pending(
    notifications: &mut Signal<Vec<crate::stores::notification::NotificationRecord>>,
) {
    let mut notifs = notifications.write();
    if let Some(n) = notifs
        .iter_mut()
        .find(|n| matches!(n.r#type, NotificationType::NeedsInput) && !n.read)
    {
        n.read = true;
    }
}

#[component]
pub fn InputRequestModal() -> Element {
    let mut free_text = use_signal(String::new);
    let mut notifications = use_notification_store();
    let mut athena = use_athena_store();

    // Find the first pending NeedsInput notification.
    let pending = notifications
        .read()
        .iter()
        .find(|n| matches!(n.r#type, NotificationType::NeedsInput) && !n.read)
        .cloned();

    let has_pending = pending.is_some();
    let prompt_text = pending
        .as_ref()
        .map(|n| n.message.clone())
        .unwrap_or_else(|| "Agent is requesting input...".to_string());

    if !has_pending {
        return rsx! {};
    }

    rsx! {
        Modal {
            title: "Agent Request",
            // Real dismiss handler (was a no-op) — backdrop click / X now
            // clears the request instead of leaving the modal stuck.
            on_close: move |_| {
                dismiss_pending(&mut notifications);
            },
            width: 440,

            div {
                style: "display: flex; flex-direction: column; gap: 12px;",

                // Agent info
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

                // Prompt — flat plaque; .modal-card owns the chrome.
                div {
                    style: "padding: 12px; border-radius: var(--radius-md); background: var(--bgSecondary); border: 1px solid var(--border);",
                    p {
                        style: "font-size: var(--text-sm); color: var(--text); margin: 0; line-height: 1.5;",
                        "{prompt_text}"
                    }
                }

                // Free text input
                div {
                    style: "display: flex; gap: 8px;",
                    input {
                        class: "field",
                        style: "flex: 1;",
                        value: "{free_text}",
                        oninput: move |e| free_text.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter && !free_text().trim().is_empty() {
                                e.prevent_default();
                                let text = free_text();
                                free_text.set(String::new());
                                let mut athena_write = athena.write();
                                athena_write.set_loading(true);
                                drop(athena_write);
                                spawn(async move {
                                    let _ = crate::tauri_bridge::athena_chat(&text).await;
                                });
                                dismiss_pending(&mut notifications);
                            }
                        },
                        placeholder: "Type a response..."
                    }
                    button {
                        class: "btn-primary",
                        style: "display: flex; align-items: center; gap: 6px;",
                        onclick: move |_| {
                            if !free_text().trim().is_empty() {
                                let text = free_text();
                                free_text.set(String::new());
                                let mut athena_write = athena.write();
                                athena_write.set_loading(true);
                                drop(athena_write);
                                spawn(async move {
                                    let _ = crate::tauri_bridge::athena_chat(&text).await;
                                });
                            }
                            // Always dismiss — previously the Send button
                            // sent the text but never marked the request read,
                            // so the modal re-appeared immediately.
                            dismiss_pending(&mut notifications);
                        },
                        IconSend { size: Some(14), color: Some("currentColor".to_string()) }
                        "Send"
                    }
                }
            }
        }
    }
}
