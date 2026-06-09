use std::cell::RefCell;
use std::rc::Rc;

use crate::components::shared::toast::{use_toast_store, Toast, ToastItem, ToastType};
use crate::stores::notification::{
    add_notification, use_notification_store, NotificationRecord, NotificationType,
};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[component]
pub fn NotificationToast() -> Element {
    let toasts = use_toast_store();
    let notifications = use_notification_store();
    let mut mounted = use_signal(|| false);

    // Store unlisten handle so it can be cleaned up on unmount.
    let unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let unlisteners_clone = unlisteners.clone();

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        // notifications:new — Show toast popup.
        let mut toast_store = toasts;
        let mut notif_store = notifications;
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

                // Map notification type to toast type.
                let toast_type = match ntype_str {
                    "warning" => ToastType::Warning,
                    "error" => ToastType::Error,
                    "success" => ToastType::Success,
                    "needsInput" => ToastType::NeedsInput,
                    "taskComplete" => ToastType::TaskComplete,
                    "taskError" => ToastType::Error,
                    _ => ToastType::Info,
                };

                // Also add to notification store.
                let notif_type = match ntype_str {
                    "warning" => NotificationType::Warning,
                    "error" => NotificationType::Error,
                    "success" => NotificationType::Success,
                    "needsInput" => NotificationType::NeedsInput,
                    "taskComplete" => NotificationType::TaskComplete,
                    "taskError" => NotificationType::TaskError,
                    _ => NotificationType::Info,
                };
                let record = NotificationRecord {
                    id: id.clone(),
                    r#type: notif_type,
                    title: title.clone(),
                    message: message.clone(),
                    source: "backend".to_string(),
                    read: false,
                    timestamp: chrono::Utc::now().timestamp(),
                };
                add_notification(&mut notif_store, record);

                // Push toast.
                let toast = Toast {
                    id,
                    toast_type,
                    title,
                    message,
                    duration_ms: 5000,
                };
                toast_store.write().push(toast);
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

    rsx! {
        div {
            class: "notification-toast-container",
            style: "position: fixed; bottom: 16px; right: 16px; z-index: 100; display: flex; flex-direction: column; gap: 10px; pointer-events: none; ",

            for toast in toasts.read().toasts.iter() {
                ToastItem { key: "{toast.id}", toast: toast.clone() }
            }
        }
    }
}
