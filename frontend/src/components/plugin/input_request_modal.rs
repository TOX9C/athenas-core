use crate::components::shared::modal::Modal;
use crate::stores::athena::use_athena_store;
use crate::stores::notification::{use_notification_store, NotificationType};
use dioxus::prelude::*;

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
            on_close: move |_| {},
            width: 440,

            div {
                style: "display: flex; flex-direction: column; gap: 12px;",

                // Agent info
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    div {
                        style: "width: 8px; height: 8px; border-radius: 50%; background: var(--accent);",
                    }
                    span {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        "Agent"
                    }
                    span {
                        style: "font-size: 9px; padding: 1px 5px; border-radius: 9999px; background: #f9731622; color: #f97316;",
                        "Needs Input"
                    }
                }

                // Prompt
                div {
                    style: "padding: 10px; border-radius: 8px; background: var(--bgTertiary); border: 1px solid var(--border);",
                    p {
                        style: "font-size: 12px; color: var(--text); margin: 0; line-height: 1.5;",
                        "{prompt_text}"
                    }
                }

                // Free text input
                div {
                    style: "display: flex; gap: 6px;",
                    input {
                        style: "flex: 1; padding: 6px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--text); font-size: 11px; outline: none;",
                        value: "{free_text}",
                        oninput: move |e| free_text.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter && !free_text().trim().is_empty() {
                                let text = free_text();
                                free_text.set(String::new());
                                // Send via athena chat
                                let mut athena_write = athena.write();
                                athena_write.set_loading(true);
                                drop(athena_write);
                                spawn(async move {
                                    let _ = crate::tauri_bridge::athena_chat(&text).await;
                                });
                                // Mark notification as read
                                if let Some(ref notif) = pending {
                                    let notif_id = notif.id.clone();
                                    let mut notifs = notifications.write();
                                    if let Some(n) = notifs.iter_mut().find(|n| n.id == notif_id) {
                                        n.read = true;
                                    }
                                }
                            }
                        },
                        placeholder: "Type a response..."
                    }
                    button {
                        style: "padding: 6px 12px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px; font-weight: 600;",
                        onclick: move |_| {
                            if !free_text().trim().is_empty() {
                                let text = free_text();
                                free_text.set(String::new());
                                spawn(async move {
                                    let _ = crate::tauri_bridge::athena_chat(&text).await;
                                });
                            }
                        },
                        "Send"
                    }
                }
            }
        }
    }
}
