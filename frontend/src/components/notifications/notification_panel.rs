use crate::components::shared::icon::IconClose;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::notification::{
    mark_notification_dismissed, use_notification_store, NotificationType,
};
use crate::tauri_bridge;
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
            class: "notification-panel pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; background: var(--bgSecondary); color: var(--text); border: var(--border);",

            // Header
            div {
                style: "padding: 12px 14px; border-bottom: var(--border); background: var(--bgSecondary); display: flex; align-items: center; gap: 8px;",
                span {
                    style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "Alerts"
                }
                if unread_count > 0 {
                    span {
                        class: "badge",
                        style: "background: var(--accentSubtle); color: var(--accent);",
                        "{unread_count}"
                    }
                }
            }

            // Great-circle rule between header and body
            div { class: "great-circle-rule" }

            // Filter tabs — segmented
            div {
                style: "display: flex; padding: 8px 12px; border-bottom: var(--border);",

                div {
                    class: "segmented",
                    style: "display: inline-flex;",

                    for tab in ["all", "unread", "alerts", "tasks"] {
                        {
                            let is_active = active_tab() == tab;
                            let tab_owned = tab.to_string();
                            rsx! {
                                button {
                                    key: "{tab}",
                                    class: if is_active { "is-active" } else { "" },
                                    style: "text-transform: capitalize;",
                                    onclick: move |_| active_tab.set(tab_owned.clone()),
                                    "{tab}"
                                }
                            }
                        }
                    }
                }
            }

            // Content
            div {
                style: "flex: 1; overflow-y: auto; overflow-x: hidden;",

                if filtered.is_empty() {
                    EmptyState {
                        kind: EmptyArt::Notifications,
                        title: "All clear".to_string(),
                        hint: Some("No notifications right now.".to_string()),
                    }
                } else {
                    for n in filtered.iter() {
                        {
                            let type_color = match &n.r#type {
                                NotificationType::Error | NotificationType::TaskError => "var(--error)",
                                NotificationType::Warning => "var(--warning)",
                                NotificationType::Success | NotificationType::TaskComplete => "var(--success)",
                                _ => "var(--accentTeal)",
                            };
                            let n_id = n.id.clone();
                            let n_title = n.title.clone();
                            let n_msg = n.message.clone();
                            let n_read = n.read;
                            let n_count = n.count;
                            let display_title = if n_count > 1 {
                                format!("{} (\u{00d7}{})", n_title, n_count)
                            } else {
                                n_title.clone()
                            };
                            let weight = if n_read { "400" } else { "600" };
                            let opacity = if n_read { "0.6" } else { "1" };
                            let title_color = if n_read { "var(--text)" } else { "var(--accent)" };
                            let unread_rule = String::new();
                            rsx! {
                                div {
                                    key: "{n_id}",
                                    class: "lit-sweep",
                                    style: "padding: 10px 12px; border-bottom: var(--border); opacity: {opacity}; display: flex; align-items: flex-start; gap: 8px; {unread_rule}",

                                    // Type dot
                                    div {
                                        style: "width: 7px; height: 7px; border-radius: var(--radius-pill); background: {type_color}; flex-shrink: 0; margin-top: 4px;",
                                    }

                                    // Text content
                                    div {
                                        style: "flex: 1; min-width: 0;",
                                        div {
                                            style: "display: flex; align-items: center; gap: 4px;",
                                            span {
                                                style: "font-size: var(--text-sm); font-weight: {weight}; color: {title_color};",
                                                "{display_title}"
                                            }
                                        }
                                        p {
                                            style: "font-size: var(--text-2xs); margin-top: 3px; color: var(--textDim);",
                                            "{n_msg}"
                                        }
                                    }

                                    // Dismiss button
                                    button {
                                        class: "icon-btn",
                                        style: "flex-shrink: 0;",
                                        onclick: move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                            let id = n_id.clone();
                                            spawn(async move {
                                                let _ = tauri_bridge::notification_dismiss(&id).await;
                                            });
                                            mark_notification_dismissed(&mut notifications, &n_id);
                                        },
                                        IconClose { size: Some(13), color: Some("currentColor".to_string()) }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Actions
            div {
                style: "display: flex; gap: 8px; padding: 8px 12px; border-top: var(--border);",
                button {
                    class: "btn-ghost btn-sm",
                    onclick: move |_| {
                        spawn(async move {
                            let _ = tauri_bridge::notification_mark_all_read().await;
                        });
                        for n in notifications.write().iter_mut() {
                            n.read = true;
                        }
                    },
                    "Mark all read"
                }
                button {
                    class: "btn-ghost btn-sm",
                    onclick: move |_| {
                        spawn(async move {
                            let _ = tauri_bridge::notification_clear_all().await;
                        });
                        notifications.write().clear();
                    },
                    "Clear all"
                }
            }
        }
    }
}
