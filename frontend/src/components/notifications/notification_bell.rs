use std::cell::RefCell;
use std::rc::Rc;

use crate::stores::notification::{
    add_notification, mark_notification_dismissed, set_notifications, use_notification_store,
    NotificationRecord, NotificationType,
};
use crate::components::shared::icon::{IconBell, IconClose};
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
    let mut notifications = use_notification_store();
    let mut mounted = use_signal(|| false);

    // Store unlisten handles so they can be cleaned up on unmount.
    let unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let unlisteners_clone = unlisteners.clone();

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        // notifications:new — Increment unread count, show badge.
        let mut new_store = notifications;
        if let Ok(u) = tauri_bridge::listen("notifications:new", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
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
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // notifications:updated — Refresh notification list.
        let mut update_store = notifications;
        if let Ok(u) = tauri_bridge::listen("notifications:updated", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(notifs_arr) = val.as_array() {
                    let records: Vec<NotificationRecord> = notifs_arr
                        .iter()
                        .filter_map(|n| {
                            let id = n.get("id").and_then(|v| v.as_str())?.to_string();
                            let title = n.get("title").and_then(|v| v.as_str())?.to_string();
                            let message = n.get("message").and_then(|v| v.as_str())?.to_string();
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
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // notifications:dismissed — Update badge count.
        let mut dismiss_store = notifications;
        if let Ok(u) = tauri_bridge::listen("notifications:dismissed", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() {
                    mark_notification_dismissed(&mut dismiss_store, id);
                }
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }
    });

    // Cleanup: unlisten all event listeners on component unmount.
    let unlisteners_drop = unlisteners.clone();
    use_drop(move || {
        for unlisten in unlisteners_drop.borrow_mut().drain(..) {
            unlisten();
        }
    });

    let unread_count: u32 = notifications.read().iter().filter(|n| !n.read).count() as u32;

    rsx! {
        div {
            class: "notification-bell",
            style: "position: relative;",

            button {
                class: "icon-btn",
                style: "position: relative;",
                "aria-label": "Notifications",
                onclick: move |_| dropdown_open.set(!dropdown_open()),

                IconBell { size: Some(16), color: Some("currentColor".to_string()) }

                if unread_count > 0 {
                    span {
                        style: "position: absolute; top: -4px; right: -4px; background: var(--accent); color: var(--bg); font-size: var(--text-2xs); font-weight: 700; padding: 1px 4px; border-radius: var(--radius-pill); min-width: 14px; text-align: center; line-height: 1.3; border: 1px solid var(--bgSecondary);",
                        "{unread_count}"
                    }
                }
            }

            if dropdown_open() {
                div {
                    style: "position: absolute; top: 100%; right: 0; width: 300px; max-height: 400px; overflow-y: auto; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md); z-index: 50; margin-top: 6px; box-shadow: 0 8px 24px rgba(0,0,0,0.4);",

                    div {
                        style: "padding: 10px 14px; border-bottom: 1px solid var(--border); font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--text); background: var(--bgTertiary); display: flex; align-items: center; justify-content: space-between;",
                        "Notifications"
                        if unread_count > 0 {
                            span {
                                class: "badge",
                                style: "background: var(--accentSubtle); color: var(--accent);",
                                "{unread_count}"
                            }
                        }
                    }

                    div {
                        if notifications.read().is_empty() {
                            div {
                                style: "padding: 22px; text-align: center; color: var(--textDim); font-size: var(--text-xs);",
                                "No notifications"
                            }
                        } else {
                            for n in notifications.read().iter().rev().take(10) {
                                {
                                    let id = n.id.clone();
                                    let title = n.title.clone();
                                    let message = n.message.clone();
                                    let is_read = n.read;
                                    let weight = if is_read { "400" } else { "600" };
                                    let type_color = match &n.r#type {
                                        NotificationType::Error | NotificationType::TaskError => "var(--error)",
                                        NotificationType::Warning => "var(--warning)",
                                        NotificationType::Success | NotificationType::TaskComplete => "var(--success)",
                                        _ => "var(--accentTeal)",
                                    };
                                    rsx! {
                                        div {
                                            key: "{id}",
                                            class: "notif-item",
                                            style: "padding: 10px; border-bottom: 1px solid var(--border); cursor: pointer; display: flex; align-items: flex-start; gap: 8px;",

                                            // Type dot
                                            div {
                                                style: "width: 7px; height: 7px; border-radius: var(--radius-pill); background: {type_color}; flex-shrink: 0; margin-top: 4px;",
                                            }

                                            // Text
                                            div {
                                                style: "flex: 1; min-width: 0;",
                                                div {
                                                    style: "font-size: var(--text-sm); font-weight: {weight}; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                    "{title}"
                                                }
                                                div {
                                                    style: "font-size: var(--text-2xs); color: var(--textDim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px;",
                                                    "{message}"
                                                }
                                            }

                                            // Dismiss button
                                            button {
                                                class: "icon-btn",
                                                style: "flex-shrink: 0;",
                                                "aria-label": "Dismiss notification",
                                                onclick: move |_| {
                                                    mark_notification_dismissed(&mut notifications, &id);
                                                },
                                                IconClose { size: Some(13), color: Some("currentColor".to_string()) }
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
