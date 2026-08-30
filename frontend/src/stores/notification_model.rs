//! Notification contracts and bounded-store constants.

use chrono::Utc;

/// Maximum number of notifications kept in memory.
pub(super) const MAX_NOTIFICATIONS: usize = 50;

/// Time window (in milliseconds) within which a duplicate notification
/// (matching title + message) is merged into the existing one and its
/// `count` is incremented instead of pushing a new entry.
pub(super) const DEDUP_WINDOW_MS: i64 = 1000;

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
    pub agent_id: Option<String>,
    pub data: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub actions: Option<Vec<serde_json::Value>>,
    pub request_id: Option<String>,
    pub event_key: Option<String>,
    pub run_id: Option<String>,
    pub pane_id: Option<String>,
    pub requires_action: bool,
    pub resolved_at: Option<i64>,
    pub dismissed_at: Option<i64>,
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
            agent_id: None,
            data: None,
            metadata: None,
            actions: None,
            request_id: None,
            event_key: None,
            run_id: None,
            pane_id: None,
            requires_action: false,
            resolved_at: None,
            dismissed_at: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    // Contract: the bounded store keeps at most MAX_NOTIFICATIONS records,
    // and duplicates inside DEDUP_WINDOW_MS merge by count rather than
    // pushing a new entry. These constants are consumed by notification.rs;
    // pin them so silent retuning breaks the UI contract visibly.
    #[test]
    fn bounds_are_pinned() {
        assert_eq!(MAX_NOTIFICATIONS, 50);
        assert_eq!(DEDUP_WINDOW_MS, 1000);
    }

    #[test]
    fn record_new_sets_defaults() {
        let record =
            NotificationRecord::new("Build failed", "see logs", NotificationType::TaskError);
        assert_eq!(record.r#type, NotificationType::TaskError);
        assert_eq!(record.title, "Build failed");
        assert_eq!(record.message, "see logs");
        assert_eq!(record.source, "frontend");
        assert!(record.id.starts_with("notif-"));
        assert_eq!(record.count, 1, "fresh record observed exactly once");
        assert!(!record.read);
        assert!(!record.requires_action);
        assert_eq!(record.resolved_at, None);
        assert_eq!(record.dismissed_at, None);
        assert!(record.timestamp > 0);
    }

    #[test]
    fn all_notification_types_have_defaults_and_equality() {
        let types = [
            NotificationType::Info,
            NotificationType::Warning,
            NotificationType::Error,
            NotificationType::Success,
            NotificationType::NeedsInput,
            NotificationType::TaskComplete,
            NotificationType::TaskError,
        ];
        assert_eq!(types.len(), 7);
        assert_eq!(NotificationType::default(), NotificationType::Info);
        // Distinctness: every variant compares unequal to every other.
        for (i, a) in types.iter().enumerate() {
            for (j, b) in types.iter().enumerate() {
                assert_eq!(a == b, i == j);
            }
        }
    }
}
