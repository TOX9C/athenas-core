use crate::stores::notification::{
    add_notification, mark_notification_dismissed, set_notifications, use_notification_store,
    NotificationRecord, NotificationType,
};
use crate::tauri_bridge;
use dioxus::prelude::*;

/// Notification data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationItem {
    pub id: String,
    pub ntype: String,
    pub title: String,
    pub message: String,
    pub timestamp: i64,
    pub read: bool,
    pub dismissed: bool,
}

#[component]
pub fn NotificationBell() -> Element {
    let mut dropdown_open = use_signal(|| false);
    let notifications = use_notification_store();
    let mut mounted = use_signal(|| false);

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        // notifications:new — Increment unread count, show badge.
        let mut new_store = notifications;
        let _ = tauri_bridge::listen("notifications:new", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let title = val
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let message = val
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let ntype_str = val.get("type").and_then(|v| v.as_str()).unwrap_or("info");
                let ntype = match ntype_str {
                    "warning" => NotificationType::Warning,
                    "error" => NotificationType::Error,
                    "success" => NotificationType::Success,
                    "needsInput" => NotificationType::NeedsInput,
                    "taskComplete" => NotificationType::TaskComplete,
                    "taskError" => NotificationType::TaskError,
                    _ => NotificationType::Info,
                };
                let record = NotificationRecord {
                    id,
                    r#type: ntype,
                    title,
                    message,
                    source: "backend".to_string(),
                    read: false,
                    timestamp: chrono::Utc::now().timestamp(),
                };
                add_notification(&mut new_store, record);
            }
        });

        // notifications:updated — Refresh notification list.
        let mut update_store = notifications;
        let _ = tauri_bridge::listen("notifications:updated", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(notifs_arr) = val.as_array() {
                    let records: Vec<NotificationRecord> = notifs_arr
                        .iter()
                        .filter_map(|n| {
                            let id = n.get("id").and_then(|v| v.as_str())?.to_string();
                            let title = n.get("title").and_then(|v| v.as_str())?.to_string();
                            let message =
                                n.get("message").and_then(|v| v.as_str())?.to_string();
                            let read = n.get("read").and_then(|v| v.as_bool()).unwrap_or(false);
                            let timestamp =
                                n.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                            let ntype_str =
                                n.get("type").and_then(|v| v.as_str()).unwrap_or("info");
                            let ntype = match ntype_str {
                                "warning" => NotificationType::Warning,
                                "error" => NotificationType::Error,
                                "success" => NotificationType::Success,
                                "needsInput" => NotificationType::NeedsInput,
                                "taskComplete" => NotificationType::TaskComplete,
                                "taskError" => NotificationType::TaskError,
                                _ => NotificationType::Info,
                            };
                            Some(NotificationRecord {
                                id,
                                r#type: ntype,
                                title,
                                message,
                                source: "backend".to_string(),
                                read,
                                timestamp,
                            })
                        })
                        .collect();
                    set_notifications(&mut update_store, records);
                }
            }
        });

        // notifications:dismissed — Update badge count.
        let mut dismiss_store = notifications;
        let _ = tauri_bridge::listen("notifications:dismissed", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() {
                    mark_notification_dismissed(&mut dismiss_store, id);
                }
            }
        });
    });

    let unread_count: u32 = notifications.read().iter().filter(|n| !n.read).count() as u32;

    rsx! {
        div {
            class: "notification-bell",
            style: "position: relative;",

            button {
                style: "padding: 4px 8px; border-radius: 4px; border: none; background: transparent; color: var(--textMuted); cursor: pointer; font-size: 11px; font-weight: 600; position: relative;",
                onclick: move |_| dropdown_open.set(!dropdown_open()),

                "NOTIF"

                if unread_count > 0 {
                    span {
                        style: "position: absolute; top: -2px; right: -2px; background: var(--error); color: #fff; font-size: 8px; font-weight: 700; padding: 1px 4px; border-radius: 9999px; min-width: 14px; text-align: center; line-height: 1.2;",
                        "{unread_count}"
                    }
                }
            }

            if dropdown_open() {
                div {
                    style: "position: absolute; top: 100%; right: 0; width: 300px; max-height: 400px; overflow-y: auto; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,0.3); z-index: 50;",

                    div {
                        style: "padding: 8px 12px; border-bottom: 1px solid var(--border); font-size: 11px; font-weight: 600; color: var(--text);",
                        "Notifications"
                    }

                    div {
                        style: "padding: 16px; text-align: center; color: var(--textDim); font-size: 10px;",
                        if unread_count > 0 {
                            "You have {unread_count} unread notifications"
                        } else {
                            "No new notifications"
                        }
                    }
                }
            }
        }
    }
}
