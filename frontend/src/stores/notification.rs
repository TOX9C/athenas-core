use chrono::Utc;
use dioxus::prelude::*;

/// Maximum number of notifications kept in memory.
const MAX_NOTIFICATIONS: usize = 50;

/// Time window (in milliseconds) within which a duplicate notification
/// (matching title + message) is merged into the existing one and its
/// `count` is incremented instead of pushing a new entry.
const DEDUP_WINDOW_MS: i64 = 1000;

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
    /// Unix epoch milliseconds. Stored as millis so the dedup window can
    /// compare in sub-second resolution.
    pub timestamp: i64,
    /// Number of times this notification has been observed. Incremented
    /// when a matching notification arrives within `DEDUP_WINDOW_MS`.
    pub count: u32,
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
            timestamp: Utc::now().timestamp_millis(),
            count: 1,
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
///
/// Identical notifications (same `title` and `message`) arriving within
/// `DEDUP_WINDOW_MS` are merged: the existing entry's `count` is
/// incremented and its `timestamp` refreshed. This prevents rapid-fire
/// backend events (e.g. connection retries) from piling up duplicates.
pub fn add_notification(
    notifications: &mut Signal<Vec<NotificationRecord>>,
    record: NotificationRecord,
) {
    let mut guard = notifications.write();
    let now = Utc::now().timestamp_millis();

    // Walk from most-recent backwards. Stop scanning once we leave the
    // dedup window — older entries are guaranteed outside it.
    for existing in guard.iter_mut().rev() {
        if now - existing.timestamp > DEDUP_WINDOW_MS {
            break;
        }
        // Exact id match still wins as a hard dedup (same event re-emitted).
        if existing.id == record.id {
            return;
        }
        if existing.title == record.title && existing.message == record.message {
            existing.count = existing.count.saturating_add(1);
            existing.timestamp = now;
            return;
        }
    }

    guard.push(record);
    // Keep only the most recent notifications
    if guard.len() > MAX_NOTIFICATIONS {
        let remove_count = guard.len() - MAX_NOTIFICATIONS;
        guard.drain(0..remove_count);
    }
}

/// Mark a notification as read by ID.
pub fn mark_notification_read(notifications: &mut Signal<Vec<NotificationRecord>>, id: &str) {
    if let Some(n) = notifications.write().iter_mut().find(|n| n.id == id) {
        n.read = true;
    }
}

/// Mark a notification as dismissed by ID (removes it from the store).
pub fn mark_notification_dismissed(notifications: &mut Signal<Vec<NotificationRecord>>, id: &str) {
    notifications.write().retain(|n| n.id != id);
}

/// Replace all notifications with a new list.
pub fn set_notifications(
    notifications: &mut Signal<Vec<NotificationRecord>>,
    records: Vec<NotificationRecord>,
) {
    *notifications.write() = records;
}
