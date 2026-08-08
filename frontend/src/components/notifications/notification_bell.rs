use crate::components::shared::icon::{IconBell, IconClose};
use crate::stores::notification::{
    mark_notification_dismissed, mark_notification_read, use_notification_store,
    NotificationRecord, NotificationType,
};
use crate::tauri_bridge;
use dioxus::prelude::*;

/// Shared open state for the root-level notification popover.
pub fn provide_notification_overlay_store() {
    use_context_provider(|| Signal::new(false));
}

fn use_notification_overlay_store() -> Signal<bool> {
    use_context::<Signal<bool>>()
}

/// Notification bell for the title bar. The popover itself is rendered by
/// [`NotificationPopover`] at the app root, outside title-bar stacking and
/// clipping contexts.
#[component]
pub fn NotificationBell() -> Element {
    let mut dropdown_open = use_notification_overlay_store();
    let notifications = use_notification_store();
    let unread_count = notifications.read().iter().filter(|n| !n.read).count();

    rsx! {
        button {
            class: "icon-btn",
            style: "position: relative;",
            "aria-label": "Notifications",
            "aria-expanded": "{dropdown_open()}",
            onclick: move |_| dropdown_open.set(!dropdown_open()),
            IconBell { size: Some(16), color: Some("currentColor".to_string()) }
            if unread_count > 0 {
                span {
                    style: "position: absolute; top: -4px; right: -4px; background: var(--accent); color: var(--bg); font-size: var(--text-2xs); font-weight: 700; padding: 1px 4px; border-radius: var(--radius-pill); min-width: 14px; text-align: center; line-height: 1.3; border: 1px solid var(--bgSecondary);",
                    "{unread_count}"
                }
            }
        }
    }
}

/// Root-level notification popover. Keeping this as a sibling of the title
/// bar guarantees `position: fixed` and the high z-index are not trapped in
/// an animated/drag-region stacking context.
#[component]
pub fn NotificationPopover() -> Element {
    let mut dropdown_open = use_notification_overlay_store();
    let mut notifications = use_notification_store();
    let visible: Vec<NotificationRecord> = notifications
        .read()
        .iter()
        .rev()
        .take(10)
        .cloned()
        .collect();
    let unread_count = notifications.read().iter().filter(|n| !n.read).count();

    if !dropdown_open() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "notification-popover pane-astrolabe-mark",
            role: "dialog",
            "aria-label": "Notifications",
            style: "position: fixed; top: calc(var(--tb-height) + 6px); right: 14px; width: min(360px, calc(100vw - 28px)); max-height: min(480px, calc(100vh - var(--tb-height) - 24px)); overflow-y: auto; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md); z-index: 10050; box-shadow: var(--shadow-lg);",

            div {
                style: "position: sticky; top: 0; z-index: 1; padding: 10px 14px; border-bottom: 1px solid var(--border); font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em; background: var(--bgSecondary); display: flex; align-items: center; justify-content: space-between; gap: 6px;",
                "Notifications"
                if unread_count > 0 {
                    span { class: "badge", style: "background: var(--accentSubtle); color: var(--accent);", "{unread_count}" }
                }
            }

            if visible.is_empty() {
                div {
                    style: "padding: 26px 22px; text-align: center; color: var(--textDim); font-size: var(--text-xs);",
                    "No notifications"
                }
            } else {
                for n in visible.iter() {
                    {
                        let id = n.id.clone();
                        let title = n.title.clone();
                        let message = n.message.clone();
                        let is_read = n.read;
                        let count = n.count;
                        let display_title = if count > 1 { format!("{} (×{})", title, count) } else { title };
                        let weight = if is_read { "400" } else { "600" };
                        let title_color = if is_read { "var(--text)" } else { "var(--accent)" };
                        let type_color = match &n.r#type {
                            NotificationType::Error | NotificationType::TaskError => "var(--error)",
                            NotificationType::Warning | NotificationType::NeedsInput => "var(--warning)",
                            NotificationType::Success | NotificationType::TaskComplete => "var(--success)",
                            _ => "var(--accentTeal)",
                        };
                        rsx! {
                            div {
                                key: "{id}",
                                class: "notif-item lit-sweep",
                                style: "padding: 10px 10px 10px 12px; border-bottom: 1px solid var(--border); cursor: pointer; display: flex; align-items: flex-start; gap: 8px;",
                                onclick: {
                                    let id = id.clone();
                                    move |_| {
                                        mark_notification_read(&mut notifications, &id);
                                        let id = id.clone();
                                        spawn(async move { let _ = tauri_bridge::notification_mark_read(&id).await; });
                                    }
                                },
                                div { style: "width: 7px; height: 7px; border-radius: var(--radius-pill); background: {type_color}; flex-shrink: 0; margin-top: 4px;" }
                                div {
                                    style: "flex: 1; min-width: 0;",
                                    div { style: "font-size: var(--text-sm); font-weight: {weight}; color: {title_color}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{display_title}" }
                                    div { style: "font-size: var(--text-2xs); color: var(--textDim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px;", "{message}" }
                                }
                                button {
                                    class: "icon-btn",
                                    style: "flex-shrink: 0;",
                                    "aria-label": "Dismiss notification",
                                    onclick: {
                                        let id = id.clone();
                                        move |event: Event<MouseData>| {
                                            event.stop_propagation();
                                            mark_notification_dismissed(&mut notifications, &id);
                                            let id = id.clone();
                                            spawn(async move { let _ = tauri_bridge::notification_dismiss(&id).await; });
                                        }
                                    },
                                    IconClose { size: Some(13), color: Some("currentColor".to_string()) }
                                }
                            }
                        }
                    }
                }
            }

            div {
                style: "position: sticky; bottom: 0; display: flex; justify-content: space-between; gap: 8px; padding: 8px 12px; border-top: 1px solid var(--border); background: var(--bgSecondary);",
                button {
                    class: "btn-ghost btn-sm",
                    onclick: move |_| {
                        spawn(async move { let _ = tauri_bridge::notification_mark_all_read().await; });
                        for n in notifications.write().iter_mut() { n.read = true; }
                    },
                    "Mark all read"
                }
                button {
                    class: "btn-ghost btn-sm",
                    onclick: move |_| {
                        spawn(async move { let _ = tauri_bridge::notification_clear_all().await; });
                        notifications.write().clear();
                        dropdown_open.set(false);
                    },
                    "Clear all"
                }
            }
        }
    }
}
