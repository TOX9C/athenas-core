use crate::components::shared::icon::{IconChevronDown, IconChevronUp, IconPlus, IconTrash};
use crate::stores::athena::{use_athena_store, AthenaMessage, MessageRole};
use crate::tauri_bridge;
use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct SessionListItem {
    id: String,
    title: String,
    updated_at: u64,
    message_count: usize,
    last_message_preview: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn do_fetch_sessions() -> Vec<SessionListItem> {
    match tauri_bridge::session_list().await {
        Ok(json) => {
            let parsed: Vec<serde_json::Value> = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("[SessionSwitcher] JSON parse error: {:?}", e).into(),
                    );
                    return Vec::new();
                }
            };
            parsed
                .iter()
                .filter_map(|v| {
                    Some(SessionListItem {
                        id: v.get("id")?.as_str()?.to_string(),
                        title: v.get("title")?.as_str()?.to_string(),
                        updated_at: v.get("updatedAt")?.as_u64()?,
                        message_count: v.get("messageCount")?.as_u64()? as usize,
                        last_message_preview: v
                            .get("lastMessagePreview")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        }
        Err(e) => {
            web_sys::console::error_1(
                &format!("[SessionSwitcher] Failed to fetch sessions: {:?}", e).into(),
            );
            Vec::new()
        }
    }
}

async fn do_load_session(
    session_id: &str,
    athena_state: &mut Signal<crate::stores::athena::AthenaState>,
) -> Result<(), String> {
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

fn format_time_ago(timestamp_ms: u64) -> String {
    let now = js_sys::Date::now() as u64; // milliseconds
    let diff = now.saturating_sub(timestamp_ms);
    let seconds = diff / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if minutes > 0 {
        format!("{}m ago", minutes)
    } else {
        "just now".to_string()
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

    // Load sessions on mount
    use_effect(move || {
        spawn(async move {
            is_loading.set(true);
            let items = do_fetch_sessions().await;
            sessions.set(items);
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

    // Clone for the click handlers that need to refresh
    let sessions_data = sessions.read().clone();
    let loading_val = is_loading();
    let is_dropdown_open = is_open();

    rsx! {
        div {
            style: "position: relative; display: inline-flex; align-items: center;",

            // Trigger button
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

            // Dropdown panel
            if is_dropdown_open {
                div {
                    style: "position: absolute; top: calc(100% + 6px); left: 0; z-index: 200; width: 280px; max-height: 360px; display: flex; flex-direction: column; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md); box-shadow: var(--shadow-md); overflow: hidden;",

                    // New chat button
                    div {
                        style: "padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                        button {
                            class: "btn-secondary btn-sm",
                            style: "width: 100%; display: flex; align-items: center; justify-content: center; gap: 6px; padding: 6px; border: 1px dashed var(--border); border-radius: var(--radius-sm); background: transparent; color: var(--text); font-family: var(--font-ui); font-size: var(--text-xs); cursor: pointer; transition: all var(--dur-fast) var(--ease);",
                            onclick: move |_| {
                                let mut athena = athena_state;
                                spawn(async move {
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
                                    // Refresh list after create
                                    let items = do_fetch_sessions().await;
                                    sessions.set(items);
                                });
                                is_open.set(false);
                            },
                            IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                            "New Chat"
                        }
                    }

                    // Session list
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
                                                    match do_load_session(&sid, &mut athena).await {
                                                        Ok(_) => {}
                                                        Err(e) => {
                                                            web_sys::console::error_1(&format!("[SessionSwitcher] {}", e).into());
                                                        }
                                                    }
                                                });
                                                is_open.set(false);
                                            },

                                            // Session info
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

                                            // Delete button
                                            button {
                                                class: "icon-btn",
                                                style: "flex-shrink: 0; opacity: 0.6; padding: 2px; border: none; background: none; cursor: pointer; color: var(--textDim); transition: opacity var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);",
                                                onclick: move |ev: Event<MouseData>| {
                                                    ev.stop_propagation();
                                                    let sid = session_id_for_delete.clone();
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
                                                        // If we deleted the active session, clear state
                                                        if athena.read().session_id.as_deref() == Some(&sid) {
                                                            athena.write().clear_messages();
                                                            athena.write().set_session_id(None);
                                                            athena.write().set_session_title(String::new());
                                                        }
                                                        // Refresh list after delete
                                                        let items = do_fetch_sessions().await;
                                                        sessions.set(items);
                                                    });
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
    }
}
