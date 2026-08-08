use chrono::Utc;
use dioxus::prelude::*;

#[path = "notification_model.rs"]
mod notification_model;

pub use notification_model::{NotificationRecord, NotificationType};
use notification_model::{DEDUP_WINDOW_MS, MAX_NOTIFICATIONS};

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
        if existing.title == record.title
            && existing.message == record.message
            && existing.source == record.source
            && existing.agent_id == record.agent_id
            && existing.request_id == record.request_id
        {
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

/// Merge persisted history into the live store without dropping events that
/// arrived while the asynchronous history request was in flight.
pub fn merge_notifications(
    notifications: &mut Signal<Vec<NotificationRecord>>,
    records: Vec<NotificationRecord>,
) {
    let mut guard = notifications.write();
    let mut by_id = std::collections::HashMap::new();
    for (index, record) in guard.iter().enumerate() {
        by_id.insert(record.id.clone(), index);
    }
    for record in records {
        if let Some(index) = by_id.get(&record.id).copied() {
            // Keep the live copy: it may have been marked read or have a
            // newer count since hydration began.
            if record.timestamp > guard[index].timestamp {
                guard[index] = record;
            }
        } else {
            by_id.insert(record.id.clone(), guard.len());
            guard.push(record);
        }
    }
    guard.sort_by_key(|record| record.timestamp);
    if guard.len() > MAX_NOTIFICATIONS {
        let remove_count = guard.len() - MAX_NOTIFICATIONS;
        guard.drain(0..remove_count);
    }
}
