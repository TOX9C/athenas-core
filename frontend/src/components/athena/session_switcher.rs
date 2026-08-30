use crate::components::shared::confirm_dialog::ConfirmDialog;
use crate::components::shared::icon::{IconChevronDown, IconChevronUp, IconPlus, IconTrash};
use crate::stores::athena::{use_athena_store, AthenaMessage, MessageRole};
use crate::tauri_bridge;
use crate::utils::session::{fetch_sessions, format_time_ago, SessionListItem};
use dioxus::prelude::*;

async fn do_load_session(
    session_id: &str,
    athena_state: &mut Signal<crate::stores::athena::AthenaState>,
) -> Result<(), String> {
    // Cancel the previous turn before replacing the visible conversation.
    // This closes the race where late chunks from session A could land in
    // session B after a user switches chats.
    let previous_request = athena_state.read().active_request_id.clone();
    if let Some(request_id) = previous_request {
        let _ = tauri_bridge::athena_cancel_stream(&request_id).await;
        athena_state.write().invalidate_active_request();
    }
    match tauri_bridge::session_get(session_id).await {
        Ok(json) => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(messages) = val.get("messages").and_then(|v| v.as_array()) {
                    let loaded: Vec<AthenaMessage> = messages
                        .iter()
                        .filter_map(|m| {
                            let role_str = m.get("role")?.as_str()?;
                            let role = if role_str.eq("user") {
                                MessageRole::User
                            } else {
                                MessageRole::Athena
                            };
                            let content = m.get("content")?.as_str()?.to_string();
                            let id = m
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let timestamp = m
                                .get("timestamp")
                                .and_then(|v| v.as_u64())
                                .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64)
                                as i64;
                            let is_error =
                                m.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
                            Some(AthenaMessage {
                                id,
                                role,
                                content,
                                timestamp,
                                is_error,
                                images: Vec::new(),
                                blocks: Vec::new(),
                            })
                        })
                        .collect();

                    let title = val
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("New Chat")
                        .to_string();

                    athena_state.write().set_messages(loaded);
                    athena_state
                        .write()
                        .set_session_id(Some(session_id.to_string()));
                    athena_state.write().set_session_title(title);
                    return Ok(());
                }
                Err("No messages field in session data".to_string())
            } else {
                Err("Failed to parse session data".to_string())
            }
        }
        Err(e) => Err(format!("Failed to load session: {:?}", e)),
    }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

#[component]
pub fn SessionSwitcher() -> Element {
    let athena_state = use_athena_store();
    let mut is_open = use_signal(|| false);
    let mut sessions: Signal<Vec<SessionListItem>> = use_signal(Vec::new);
    let mut is_loading = use_signal(|| false);
    // Session id awaiting delete confirmation (deleting a chat is destructive
    // and irreversible — the full transcript is dropped).
    let mut confirm_delete = use_signal(|| None::<String>);

    // Load sessions on mount.
    use_effect(move || {
        spawn(async move {
            is_loading.set(true);
            match fetch_sessions().await {
                Ok(items) => sessions.set(items),
                Err(error) => {
                    web_sys::console::error_1(&format!("[SessionSwitcher] {error}").into());
                }
            }
            is_loading.set(false);
        });
    });

    let current_id = athena_state.read().session_id.clone();
    let current_title = athena_state.read().session_title.clone();
    let display_title = if current_title.is_empty() {
        "New Chat".to_string()
    } else {
        current_title
    };

    // Clone for the click handlers that need to refresh.
    let sessions_data = sessions.read().clone();
    let loading_val = is_loading();
    let is_dropdown_open = is_open();

    rsx! {
        div {
            style: "position: relative; display: inline-flex; align-items: center;",

            // Trigger button.
            button {
                class: "btn-ghost btn-sm",
                style: "display: flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: var(--radius-sm); border: none; background: transparent; color: var(--text); font-family: var(--font-ui); font-size: var(--text-xs); cursor: pointer; transition: background var(--dur-fast) var(--ease);",
                onclick: move |_| { is_open.toggle(); },

                span {
                    style: "max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{display_title}"
                }

                if is_dropdown_open {
                    IconChevronUp { size: Some(12), color: Some("currentColor".to_string()) }
                } else {
                    IconChevronDown { size: Some(12), color: Some("currentColor".to_string()) }
                }
            }

            // Dropdown panel.
            if is_dropdown_open {
                div {
                    style: "position: absolute; top: calc(100% + 6px); left: 0; z-index: 200; width: 280px; max-height: 360px; display: flex; flex-direction: column; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md); box-shadow: var(--shadow-md); overflow: hidden;",

                    // New chat button.
                    div {
                        style: "padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                        button {
                            class: "btn-secondary btn-sm",
                            style: "width: 100%; display: flex; align-items: center; justify-content: center; gap: 6px; padding: 6px; border: 1px dashed var(--border); border-radius: var(--radius-sm); background: transparent; color: var(--text); font-family: var(--font-ui); font-size: var(--text-xs); cursor: pointer; transition: all var(--dur-fast) var(--ease);",
                            onclick: move |_| {
                                let mut athena = athena_state;
                                spawn(async move {
                                    let request_id = athena.read().active_request_id.clone();
                                    if let Some(request_id) = request_id {
                                        let _ = tauri_bridge::athena_cancel_stream(&request_id).await;
                                        athena.write().invalidate_active_request();
                                    }
                                    match tauri_bridge::session_create(Some("New Chat")).await {
                                        Ok(json) => {
                                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
                                                if let Some(real_id) = val.get("id").and_then(|v| v.as_str()) {
                                                    athena.write().clear_messages();
                                                    athena.write().set_session_id(Some(real_id.to_string()));
                                                    athena.write().set_session_title("New Chat".to_string());
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            web_sys::console::error_1(&format!("[SessionSwitcher] Failed to create session: {:?}", e).into());
                                        }
                                    }
                                    match fetch_sessions().await {
                                        Ok(items) => sessions.set(items),
                                        Err(error) => web_sys::console::error_1(
                                            &format!("[SessionSwitcher] {error}").into(),
                                        ),
                                    }
                                });
                                is_open.set(false);
                            },
                            IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                            "New Chat"
                        }
                    }

                    // Session list.
                    div {
                        style: "flex: 1; overflow-y: auto; padding: 4px 0;",

                        if loading_val {
                            div {
                                style: "padding: 12px; text-align: center; color: var(--textDim); font-size: 11px;",
                                "Loading sessions…"
                            }
                        } else if sessions_data.is_empty() {
                            div {
                                style: "padding: 12px; text-align: center; color: var(--textDim); font-size: 11px;",
                                "No saved sessions. Start chatting!"
                            }
                        } else {
                            for session in sessions_data.iter() {
                                {
                                    let session_id_for_click = session.id.clone();
                                    let session_id_for_delete = session.id.clone();
                                    let is_active = current_id.as_deref() == Some(&session.id);
                                    let title_color = if is_active { "var(--accent)" } else { "var(--text)" };
                                    let row_style = "display: flex; align-items: center; gap: 6px; padding: 8px 10px 8px 12px; cursor: pointer; transition: color var(--dur-fast) var(--ease);".to_string();
                                    let preview_text = if session.last_message_preview.is_empty() {
                                        "No messages".to_string()
                                    } else {
                                        session.last_message_preview.chars().take(40).collect::<String>()
                                    };
                                    let updated = format_time_ago(session.updated_at);
                                    let msg_count = session.message_count;
                                    let title = session.title.clone();

                                    rsx! {
                                        div {
                                            key: "{session.id}",
                                            style: "{row_style}",
                                            onclick: move |_| {
                                                let sid = session_id_for_click.clone();
                                                let mut athena = athena_state;
                                                spawn(async move {
                                                    if let Err(error) = do_load_session(&sid, &mut athena).await {
                                                        web_sys::console::error_1(&format!("[SessionSwitcher] {error}").into());
                                                    }
                                                });
                                                is_open.set(false);
                                            },

                                            div {
                                                style: "flex: 1; min-width: 0;",
                                                div {
                                                    style: "font-size: var(--text-xs); font-weight: 500; color: {title_color}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                                    "{title}"
                                                }
                                                div {
                                                    style: "display: flex; justify-content: space-between; align-items: center; margin-top: 2px;",
                                                    div {
                                                        style: "font-size: var(--text-2xs); color: var(--textDim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px;",
                                                        "{preview_text}"
                                                    }
                                                    div {
                                                        style: "font-size: var(--text-2xs); color: var(--textDim); white-space: nowrap; margin-left: 4px;",
                                                        "{msg_count} · {updated}"
                                                    }
                                                }
                                            }

                                            // Delete button — opens a confirmation dialog.
                                            button {
                                                class: "icon-btn",
                                                style: "flex-shrink: 0; opacity: 0.6; padding: 2px; border: none; background: none; cursor: pointer; color: var(--textDim); transition: opacity var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);",
                                                onclick: move |ev: Event<MouseData>| {
                                                    ev.stop_propagation();
                                                    confirm_delete.set(Some(session_id_for_delete.clone()));
                                                },
                                                title: "Delete session",
                                                IconTrash { size: Some(12), color: Some("currentColor".to_string()) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Confirm before deleting a chat — the transcript is dropped permanently.
        if let Some(pending_id) = confirm_delete() {
            {
                let pending_title = sessions
                    .read()
                    .iter()
                    .find(|s| s.id == pending_id)
                    .map(|s| s.title.clone())
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "this chat".to_string());
                let mut confirm_delete_set = confirm_delete;
                rsx! {
                    ConfirmDialog {
                        title: "Delete chat".to_string(),
                        message: format!("Delete \"{pending_title}\"? Its transcript will be permanently removed."),
                        confirm_label: "Delete Chat".to_string(),
                        on_cancel: move |_| confirm_delete_set.set(None),
                        on_confirm: move |_| {
                            confirm_delete_set.set(None);
                            let sid = pending_id.clone();
                            let mut athena = athena_state;
                            spawn(async move {
                                match tauri_bridge::session_delete(&sid).await {
                                    Ok(_) => {
                                        web_sys::console::log_1(&format!("[SessionSwitcher] Deleted session {}", sid).into());
                                    }
                                    Err(e) => {
                                        web_sys::console::error_1(&format!("[SessionSwitcher] Failed to delete session: {:?}", e).into());
                                    }
                                }
                                if athena.read().session_id.as_deref() == Some(&sid) {
                                    athena.write().clear_messages();
                                    athena.write().set_session_id(None);
                                    athena.write().set_session_title(String::new());
                                }
                                match fetch_sessions().await {
                                    Ok(items) => sessions.set(items),
                                    Err(error) => web_sys::console::error_1(
                                        &format!("[SessionSwitcher] {error}").into(),
                                    ),
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}
