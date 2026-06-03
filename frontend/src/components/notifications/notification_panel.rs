use crate::stores::notification::{
    mark_notification_dismissed, use_notification_store, NotificationType,
};
use dioxus::prelude::*;

#[component]
pub fn NotificationPanel() -> Element {
    let mut notifications = use_notification_store();
    let mut active_tab = use_signal(|| "all".to_string());

    let filtered: Vec<_> = notifications
        .read()
        .iter()
        .filter(|n| match active_tab().as_str() {
            "unread" => !n.read,
            "alerts" => matches!(
                n.r#type,
                NotificationType::Error | NotificationType::Warning | NotificationType::TaskError
            ),
            "tasks" => matches!(
                n.r#type,
                NotificationType::TaskComplete | NotificationType::TaskError
            ),
            _ => true,
        })
        .cloned()
        .collect();

    let unread_count = notifications.read().iter().filter(|n| !n.read).count();

    rsx! {
        div {
            class: "notification-panel",
            style: "display: flex; flex-direction: column; height: 100%; background: var(--bg); color: var(--text);",

            // Header
            div {
                style: "padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); display: flex; align-items: center; justify-content: space-between;",
                span {
                    style: "font-size: 13px; font-weight: 600; color: var(--text);",
                    "Alerts"
                }
                if unread_count > 0 {
                    span {
                        style: "font-size: 9px; padding: 1px 5px; border-radius: 9999px; background: var(--error); color: #fff;",
                        "{unread_count}"
                    }
                }
            }

            // Filter tabs
            div {
                style: "display: flex; gap: 4px; padding: 6px 12px; border-bottom: 1px solid var(--border);",

                for tab in ["all", "unread", "alerts", "tasks"] {
                    {
                        let is_active = active_tab() == tab;
                        let bg = if is_active { "var(--accent)" } else { "transparent" };
                        let color = if is_active { "#0b0e13" } else { "var(--textDim)" };
                        let tab_owned = tab.to_string();
                        rsx! {
                            button {
                                key: "{tab}",
                                style: "padding: 3px 8px; border-radius: 4px; border: none; background: {bg}; color: {color}; cursor: pointer; font-size: 10px; font-weight: 500; text-transform: capitalize;",
                                onclick: move |_| active_tab.set(tab_owned.clone()),
                                "{tab}"
                            }
                        }
                    }
                }
            }

            // Content
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px;",

                if filtered.is_empty() {
                    div {
                        style: "text-align: center; padding: 32px; color: var(--textDim); font-size: 11px;",
                        "No notifications"
                    }
                } else {
                    for n in filtered.iter() {
                        {
                            let type_color = match &n.r#type {
                                NotificationType::Error | NotificationType::TaskError => "var(--error)",
                                NotificationType::Warning => "var(--warning)",
                                NotificationType::Success | NotificationType::TaskComplete => "var(--success)",
                                _ => "var(--accent)",
                            };
                            let n_id = n.id.clone();
                            let n_title = n.title.clone();
                            let n_msg = n.message.clone();
                            let n_read = n.read;
                            let weight = if n_read { "400" } else { "600" };
                            let opacity = if n_read { "0.6" } else { "1" };
                            rsx! {
                                div {
                                    key: "{n_id}",
                                    style: "padding: 8px; border-bottom: 1px solid var(--border); opacity: {opacity}; display: flex; align-items: flex-start; gap: 8px;",

                                    // Type dot
                                    div {
                                        style: "width: 7px; height: 7px; border-radius: 50%; background: {type_color}; flex-shrink: 0; margin-top: 3px;",
                                    }

                                    // Text content
                                    div {
                                        style: "flex: 1; min-width: 0;",
                                        div {
                                            style: "display: flex; align-items: center; gap: 4px;",
                                            span {
                                                style: "font-size: 11px; font-weight: {weight}; color: var(--text);",
                                                "{n_title}"
                                            }
                                        }
                                        p {
                                            style: "font-size: 9px; margin-top: 2px; color: var(--textDim);",
                                            "{n_msg}"
                                        }
                                    }

                                    // Dismiss button
                                    button {
                                        style: "flex-shrink: 0; padding: 2px 5px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 11px; line-height: 1;",
                                        onclick: move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                            mark_notification_dismissed(&mut notifications, &n_id);
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Actions
            div {
                style: "display: flex; gap: 8px; padding: 6px 12px; border-top: 1px solid var(--border);",
                button {
                    style: "padding: 4px 8px; border-radius: 4px; border: none; background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 10px;",
                    onclick: move |_| {
                        for n in notifications.write().iter_mut() {
                            n.read = true;
                        }
                    },
                    "Mark all read"
                }
                button {
                    style: "padding: 4px 8px; border-radius: 4px; border: none; background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 10px;",
                    onclick: move |_| {
                        notifications.write().clear();
                    },
                    "Clear all"
                }
            }
        }
    }
}
