use chrono::Utc;
use dioxus::prelude::*;

/// Type of notification.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum NotificationType {
    #[default]
    Info,
    Warning,
    Error,
    Success,
    NeedsInput,
    TaskComplete,
    TaskError,
}

/// A single notification record.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationRecord {
    pub id: String,
    pub r#type: NotificationType,
    pub title: String,
    pub message: String,
    pub source: String,
    pub read: bool,
    pub timestamp: i64,
}

impl NotificationRecord {
    pub fn new(title: &str, message: &str, r#type: NotificationType) -> Self {
        Self {
            id: format!("notif-{}", Utc::now().timestamp_millis()),
            r#type,
            title: title.to_string(),
            message: message.to_string(),
            source: "frontend".to_string(),
            read: false,
            timestamp: Utc::now().timestamp(),
        }
    }
}

impl Default for NotificationRecord {
    fn default() -> Self {
        Self::new("", "", NotificationType::Info)
    }
}

/// Global notification store.
pub fn use_notification_store() -> Signal<Vec<NotificationRecord>> {
    use_context::<Signal<Vec<NotificationRecord>>>()
}

/// Initialize notification store.
pub fn provide_notification_store() {
    use_context_provider(|| Signal::new(Vec::<NotificationRecord>::new()));
}

// -- Mutators for event handling -------------------------------------------

/// Add a new notification to the store.
pub fn add_notification(notifications: &mut Signal<Vec<NotificationRecord>>, record: NotificationRecord) {
    notifications.write().push(record);
}

/// Mark a notification as read by ID.
pub fn mark_notification_read(notifications: &mut Signal<Vec<NotificationRecord>>, id: &str) {
    if let Some(n) = notifications.write().iter_mut().find(|n| n.id == id) {
        n.read = true;
    }
}

/// Mark a notification as dismissed by ID.
pub fn mark_notification_dismissed(notifications: &mut Signal<Vec<NotificationRecord>>, id: &str) {
    if let Some(n) = notifications.write().iter_mut().find(|n| n.id == id) {
        n.read = true;
    }
}

/// Replace all notifications with a new list.
pub fn set_notifications(notifications: &mut Signal<Vec<NotificationRecord>>, records: Vec<NotificationRecord>) {
    *notifications.write() = records;
}
