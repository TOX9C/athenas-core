use std::cell::RefCell;
use std::rc::Rc;

use crate::components::shared::toast::{use_toast_store, Toast, ToastType};
use crate::stores::notification::{
    add_notification, mark_notification_read, use_notification_store, NotificationRecord,
    NotificationType,
};
use crate::tauri_bridge;
use dioxus::prelude::*;

fn notification_type(value: Option<&str>) -> NotificationType {
    match value.unwrap_or("info") {
        "warning" => NotificationType::Warning,
        "error" => NotificationType::Error,
        "success" => NotificationType::Success,
        "needs_input" | "needsInput" => NotificationType::NeedsInput,
        "task_complete" | "taskComplete" => NotificationType::TaskComplete,
        "task_error" | "taskError" => NotificationType::TaskError,
        _ => NotificationType::Info,
    }
}

fn record_from_value(value: &serde_json::Value) -> Option<NotificationRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Notification")
        .to_string();
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Some(NotificationRecord {
        id,
        r#type: notification_type(value.get("type").and_then(|v| v.as_str())),
        title,
        message,
        source: value
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("backend")
            .to_string(),
        agent_id: value
            .get("agentId")
            .or_else(|| value.get("agent_id"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        data: value.get("data").cloned().filter(|v| !v.is_null()),
        metadata: value.get("metadata").cloned().filter(|v| !v.is_null()),
        actions: value
            .get("actions")
            .and_then(|v| v.as_array())
            .map(|items| items.to_vec()),
        request_id: value
            .get("requestId")
            .or_else(|| value.get("request_id"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        event_key: value
            .get("eventKey")
            .or_else(|| value.get("event_key"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        run_id: value
            .get("runId")
            .or_else(|| value.get("run_id"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        pane_id: value
            .get("paneId")
            .or_else(|| value.get("pane_id"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        requires_action: value
            .get("requiresAction")
            .or_else(|| value.get("requires_action"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        resolved_at: value
            .get("resolvedAt")
            .or_else(|| value.get("resolved_at"))
            .and_then(|v| v.as_i64()),
        dismissed_at: value
            .get("dismissedAt")
            .or_else(|| value.get("dismissed_at"))
            .and_then(|v| v.as_i64()),
        read: value.get("read").and_then(|v| v.as_bool()).unwrap_or(false),
        timestamp: value
            .get("timestamp")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                value
                    .get("timestamp")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.max(0) as u64)
            })
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64)
            as i64,
        count: 1,
    })
}

fn toast_type(ntype: &NotificationType) -> ToastType {
    match ntype {
        NotificationType::Warning | NotificationType::NeedsInput => ToastType::Warning,
        NotificationType::Error | NotificationType::TaskError => ToastType::Error,
        NotificationType::Success => ToastType::Success,
        NotificationType::TaskComplete => ToastType::TaskComplete,
        NotificationType::Info => ToastType::Info,
    }
}

enum NotificationBusEvent {
    New(serde_json::Value),
    MarkRead(String),
    MarkAllRead,
    Resolved(String),
    Dismissed(String),
    Clear,
}

/// Single notification event consumer. The bell and panel are render-only;
/// this prevents duplicate history rows and duplicate toasts for one event.
#[component]
pub fn NotificationToast() -> Element {
    let toasts = use_toast_store();
    let notifications = use_notification_store();
    let mut mounted = use_signal(|| false);
    let unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    let dispatcher = use_coroutine(
        move |mut rx: UnboundedReceiver<NotificationBusEvent>| async move {
            let mut toasts = toasts;
            let mut notifications = notifications;
            while let Ok(event) = rx.recv().await {
                match event {
                    NotificationBusEvent::New(value) => {
                        let Some(record) = record_from_value(&value) else {
                            continue;
                        };
                        let ntype = record.r#type.clone();
                        let toast = Toast {
                            id: record.id.clone(),
                            toast_type: toast_type(&ntype),
                            title: record.title.clone(),
                            message: record.message.clone(),
                            // Toasts are transient attention cues. Durable
                            // history and unresolved input state live in the
                            // notification store, not in this bottom-right UI.
                            duration_ms: match ntype {
                                NotificationType::NeedsInput => 2500,
                                NotificationType::TaskError | NotificationType::Error => 3000,
                                NotificationType::TaskComplete
                                | NotificationType::Success
                                | NotificationType::Warning => 2000,
                                NotificationType::Info => 1800,
                            },
                        };
                        let id = record.id.clone();
                        add_notification(&mut notifications, record);
                        if !toasts.read().toasts.iter().any(|t| t.id == id) {
                            toasts.write().push(toast);
                        }
                    }
                    NotificationBusEvent::MarkRead(id) => {
                        mark_notification_read(&mut notifications, &id);
                    }
                    NotificationBusEvent::MarkAllRead => {
                        for record in notifications.write().iter_mut() {
                            record.read = true;
                        }
                    }
                    NotificationBusEvent::Resolved(id) => {
                        if let Some(record) = notifications.write().iter_mut().find(|n| n.id == id)
                        {
                            record.read = true;
                            record.resolved_at = Some(chrono::Utc::now().timestamp_millis());
                        }
                    }
                    NotificationBusEvent::Dismissed(id) => {
                        notifications.write().retain(|record| record.id != id);
                        toasts.write().remove(&id);
                    }
                    NotificationBusEvent::Clear => {
                        notifications.write().clear();
                        toasts.write().toasts.clear();
                    }
                }
            }
        },
    );

    let listeners_for_effect = unlisteners.clone();
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        let mut hydrate_store = notifications;
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(raw) = tauri_bridge::notification_history(Some(50)).await {
                if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
                    let records = values.iter().filter_map(record_from_value).collect();
                    crate::stores::notification::merge_notifications(&mut hydrate_store, records);
                }
            }
        });

        let new_dispatcher = dispatcher;
        if let Ok(unlisten) = tauri_bridge::listen("notifications:new", move |payload: String| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) {
                new_dispatcher.send(NotificationBusEvent::New(value));
            }
        }) {
            listeners_for_effect.borrow_mut().push(unlisten);
        }

        let update_dispatcher = dispatcher;
        if let Ok(unlisten) =
            tauri_bridge::listen("notifications:updated", move |payload: String| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) {
                    if value.get("all").and_then(|v| v.as_bool()).unwrap_or(false) {
                        update_dispatcher.send(NotificationBusEvent::MarkAllRead);
                    } else if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                        update_dispatcher.send(NotificationBusEvent::MarkRead(id.to_string()));
                    }
                }
            })
        {
            listeners_for_effect.borrow_mut().push(unlisten);
        }

        let resolved_dispatcher = dispatcher;
        if let Ok(unlisten) =
            tauri_bridge::listen("notifications:resolved", move |payload: String| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) {
                    if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                        resolved_dispatcher.send(NotificationBusEvent::Resolved(id.to_string()));
                    }
                }
            })
        {
            listeners_for_effect.borrow_mut().push(unlisten);
        }

        let dismissed_dispatcher = dispatcher;
        if let Ok(unlisten) =
            tauri_bridge::listen("notifications:dismissed", move |payload: String| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) {
                    if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                        dismissed_dispatcher.send(NotificationBusEvent::Dismissed(id.to_string()));
                    }
                }
            })
        {
            listeners_for_effect.borrow_mut().push(unlisten);
        }

        let clear_dispatcher = dispatcher;
        if let Ok(unlisten) =
            tauri_bridge::listen("notifications:cleared", move |_payload: String| {
                clear_dispatcher.send(NotificationBusEvent::Clear);
            })
        {
            listeners_for_effect.borrow_mut().push(unlisten);
        }
    });

    let listeners_for_drop = unlisteners.clone();
    use_drop(move || {
        for unlisten in listeners_for_drop.borrow_mut().drain(..) {
            unlisten();
        }
    });

    rsx! {}
}
