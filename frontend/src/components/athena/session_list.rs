use crate::components::shared::icon::{IconPlus, IconRefresh};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::athena::{use_athena_store, AthenaMessage, MessageRole};
use crate::tauri_bridge;
use dioxus::prelude::*;

/// A single chat session as returned by the backend list endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionListItem {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
    pub last_message_preview: String,
}

/// Load the session list from the backend.
async fn fetch_sessions() -> Vec<SessionListItem> {
    match tauri_bridge::session_list().await {
        Ok(json) => {
            let parsed: Vec<serde_json::Value> = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            };
            parsed
                .iter()
                .filter_map(|v| {
                    Some(SessionListItem {
                        id: v.get("id")?.as_str()?.to_string(),
                        title: v.get("title")?.as_str()?.to_string(),
                        created_at: v.get("createdAt")?.as_u64()?,
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
        Err(_) => Vec::new(),
    }
}

/// Load a specific session into the athena store.
async fn load_session(
    session_id: &str,
    athena_state: &mut Signal<crate::stores::athena::AthenaState>,
) {
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
                    let id = session_id.to_string();

                    athena_state.write().set_messages(loaded);
                    athena_state.write().set_session_id(Some(id));
                    athena_state.write().set_session_title(title);
                }
            }
        }
        Err(e) => {
            web_sys::console::warn_1(
                &format!("[SessionList] Failed to load session: {:?}", e).into(),
            );
        }
    }
}

/// Format a Unix timestamp (milliseconds) into a relative "time ago" string.
fn format_time_ago(timestamp_ms: u64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let diff = if now > timestamp_ms {
        now - timestamp_ms
    } else {
        0
    };
    let minutes = diff / 60;
    let hours = diff / 3600;
    let days = diff / 86400;

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

#[component]
pub fn SessionList() -> Element {
    let mut sessions = use_signal(Vec::<SessionListItem>::new);
    let mut loading = use_signal(|| false);
    let mut athena_state = use_athena_store();

    // Load sessions on mount
    use_effect(move || {
        loading.set(true);
        spawn(async move {
            let items = fetch_sessions().await;
            sessions.set(items);
            loading.set(false);
        });
    });

    let current_session_id = athena_state.read().session_id.clone();

    // Build a list of session render data outside the RSX
    let session_data: Vec<SessionListItem> = sessions.read().clone();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; overflow: hidden;",

            // Header
            div {
                style: "padding: 8px 10px; border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between; flex-shrink: 0;",
                span {
                    style: "font-family: var(--font-display); font-size: 14px; font-weight: 600; letter-spacing: 0.01em; color: var(--text);",
                    "Sessions"
                }
                button {
                    class: "icon-btn",
                    title: "New chat",
                    onclick: move |_| {
                        let new_id = uuid::Uuid::new_v4().to_string();
                        athena_state.write().clear_messages();
                        athena_state.write().set_session_id(Some(new_id));
                        athena_state.write().set_session_title("New Chat");
                    },
                    IconPlus { size: Some(16), color: Some("currentColor".to_string()) }
                }
            }

            // Refresh button row
            div {
                style: "padding: 6px 10px; display: flex; gap: 4px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                button {
                    class: "icon-btn",
                    title: "Refresh sessions",
                    onclick: move |_| {
                        loading.set(true);
                        spawn(async move {
                            let items = fetch_sessions().await;
                            sessions.set(items);
                            loading.set(false);
                        });
                    },
                    IconRefresh { size: Some(15), color: Some("currentColor".to_string()) }
                }
            }

            // Session items
            div {
                style: "flex: 1; overflow-y: auto;",

                if loading() {
                    div {
                        style: "padding: 16px; text-align: center; color: var(--textDim); font-size: 10px;",
                        "Loading sessions..."
                    }
                } else if session_data.is_empty() {
                    EmptyState {
                        kind: EmptyArt::Sessions,
                        title: "No sessions".to_string(),
                        hint: Some("Past conversations will appear here.".to_string()),
                    }
                } else {
                    for session in session_data.iter() {
                        {
                            let sid = session.id.clone();
                            let sid_for_click = sid.clone();
                            let is_active = current_session_id.as_deref() == Some(&sid);
                            let title_color = if is_active { "var(--accent)" } else { "var(--text)" };
                            let row_style = if is_active {
                                "padding: 10px 12px 10px 9px; border-bottom: 1px solid var(--border); border-left: 3px solid var(--accent); background: var(--bgHover); cursor: pointer; transition: background var(--dur-fast) var(--ease);"
                            } else {
                                "padding: 10px 12px 10px 12px; border-bottom: 1px solid var(--border); border-left: 3px solid transparent; cursor: pointer; transition: background var(--dur-fast) var(--ease);"
                            };
                            let title = session.title.clone();
                            let msg_count = session.message_count;
                            let updated = format_time_ago(session.updated_at);
                            let preview = if session.last_message_preview.is_empty() {
                                "No messages".to_string()
                            } else {
                                let preview_text: String = session
                                    .last_message_preview
                                    .chars()
                                    .take(50)
                                    .collect();
                                preview_text
                            };
                            let athena_state_click = athena_state;

                            rsx! {
                                div {
                                    key: "{session.id}",
                                    class: "session-row",
                                    style: row_style,
                                    onclick: move |_| {
                                        let sid = sid_for_click.clone();
                                        let mut athena = athena_state_click;
                                        spawn(async move {
                                            load_session(&sid, &mut athena).await;
                                        });
                                    },
                                    div {
                                        style: "font-size: var(--text-xs); font-weight: 500; color: {title_color};",
                                        "{title}"
                                    }
                                    div {
                                        style: "display: flex; justify-content: space-between; align-items: center; margin-top: 2px;",
                                        div {
                                            style: "font-size: var(--text-xs); color: var(--textDim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 120px;",
                                            "{preview}"
                                        }
                                        div {
                                            style: "font-size: var(--text-2xs); color: var(--textDim); white-space: nowrap; margin-left: 4px;",
                                            "{msg_count} msg - {updated}"
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
