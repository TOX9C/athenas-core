use crate::components::shared::toast::{Toast, ToastItem, ToastType, use_toast_store};
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

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        // notifications:new — Show toast popup.
        let mut toast_store = toasts;
        let mut notif_store = notifications;
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
        });
    });

    rsx! {
        div {
            class: "notification-toast-container",
            style: "position: fixed; bottom: 16px; right: 16px; z-index: 100; display: flex; flex-direction: column; gap: 8px; pointer-events: none;",

            for toast in toasts.read().toasts.iter() {
                ToastItem { key: "{toast.id}", toast: toast.clone() }
            }
        }
    }
}
