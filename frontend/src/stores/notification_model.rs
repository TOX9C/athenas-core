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
