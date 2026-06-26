use std::sync::Arc;
use thiserror::Error;

/// Type of notification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    Info,
    Warning,
    Error,
    Success,
    NeedsInput,
    TaskComplete,
    TaskError,
}

/// An action button on a notification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

/// Event data for creating a notification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationEvent {
    pub r#type: NotificationType,
    pub title: String,
    pub message: String,
    pub source: String,
    pub agent_id: Option<String>,
    pub data: Option<serde_json::Value>,
    pub timestamp: u64,
    pub metadata: Option<serde_json::Value>,
    pub actions: Option<Vec<NotificationAction>>,
    pub request_id: Option<String>,
}

/// A persisted notification record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationRecord {
    pub id: String,
    pub r#type: NotificationType,
    pub title: String,
    pub message: String,
    pub source: String,
    pub agent_id: Option<String>,
    pub data: Option<serde_json::Value>,
    pub timestamp: u64,
    pub metadata: Option<serde_json::Value>,
    pub actions: Option<Vec<NotificationAction>>,
    pub request_id: Option<String>,
    pub read: bool,
    pub dismissed_at: Option<u64>,
}

/// Options for filtering notification history.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HistoryOptions {
    pub limit: Option<usize>,
    pub unread_only: Option<bool>,
    pub r#type: Option<NotificationType>,
    pub source: Option<String>,
}

/// Count summary for notifications.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationCounts {
    pub total: usize,
    pub unread: usize,
    pub by_type: NotificationTypeCounts,
}

/// Per-type notification counts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationTypeCounts {
    pub info: usize,
    pub warning: usize,
    pub error: usize,
    pub success: usize,
    pub needs_input: usize,
    pub task_complete: usize,
    pub task_error: usize,
}

const MAX_HISTORY: usize = 500;

/// Errors for the notification service.
#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("Notification not found: {0}")]
    NotFound(String),
}

/// Thread-safe notification service.
pub struct NotificationService {
    history: Arc<parking_lot::RwLock<Vec<NotificationRecord>>>,
    event_emitter:
        Arc<parking_lot::Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
    pending_events: Arc<parking_lot::Mutex<Vec<(String, serde_json::Value)>>>,
}

impl std::fmt::Debug for NotificationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationService")
            .field("history", &"<RwLock<Vec>>")
            .field("event_emitter", &"<Option>")
            .finish()
    }
}

impl Clone for NotificationService {
    fn clone(&self) -> Self {
        Self {
            history: Arc::clone(&self.history),
            event_emitter: Arc::clone(&self.event_emitter),
            pending_events: Arc::clone(&self.pending_events),
        }
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationService {
    pub fn new() -> Self {
        Self {
            history: Arc::new(parking_lot::RwLock::new(Vec::new())),
            event_emitter: Arc::new(parking_lot::Mutex::new(None)),
            pending_events: Arc::new(parking_lot::Mutex::new(Vec::new())),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        let mut guard = self.event_emitter.lock();
        *guard = Some(Box::new(emitter));
    }

    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        let guard = self.event_emitter.lock();
        if let Some(ref emitter) = *guard {
            emitter(channel, data);
            drop(guard);
            return;
        }
        drop(guard);
        log::debug!("[notification] {} -> {}", channel, data);
        let mut pending = self.pending_events.lock();
        pending.push((channel.to_string(), data.clone()));
    }

    /// Flush any events that were queued before the event emitter was wired.
    pub fn flush_pending_events(&self) {
        let mut pending = self.pending_events.lock();
        if pending.is_empty() {
            return;
        }
        let guard = self.event_emitter.lock();
        if let Some(ref emitter) = *guard {
            for (channel, data) in pending.drain(..) {
                emitter(&channel, &data);
            }
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| {
                log::warn!("System clock error");
                std::time::Duration::default()
            })
            .as_millis() as u64
    }

    fn generate_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("notif-{}-{}", Self::now(), n)
    }

    /// Push a simple notification with a title and message.
    pub fn notify(
        &self,
        ntype: NotificationType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> NotificationRecord {
        let event = NotificationEvent {
            r#type: ntype,
            title: title.into(),
            message: message.into(),
            source: "backend".to_string(),
            agent_id: None,
            data: None,
            timestamp: Self::now(),
            metadata: None,
            actions: None,
            request_id: None,
        };
        self.push_notification(event)
    }

    /// Push a new notification to the history.
    pub fn push_notification(&self, event: NotificationEvent) -> NotificationRecord {
        let record = NotificationRecord {
            id: Self::generate_id(),
            r#type: event.r#type,
            title: event.title,
            message: event.message,
            source: event.source,
            agent_id: event.agent_id,
            data: event.data,
            timestamp: event.timestamp,
            metadata: event.metadata,
            actions: event.actions,
            request_id: event.request_id,
            read: false,
            dismissed_at: None,
        };

        let mut history = self.history.write();
        history.push(record.clone());

        // Trim to max history
        if history.len() > MAX_HISTORY {
            let excess = history.len() - MAX_HISTORY;
            history.drain(0..excess);
        }

        drop(history);

        self.emit_event(
            "notifications:new",
            &serde_json::json!({
                "id": record.id,
                "type": record.r#type,
                "title": record.title,
                "message": record.message,
                "source": record.source,
                "agentId": record.agent_id,
                "timestamp": record.timestamp,
            }),
        );

        record
    }

    /// Get notification history with optional filtering.
    pub fn get_history(&self, options: Option<&HistoryOptions>) -> Vec<NotificationRecord> {
        let history = self.history.read();
        let mut results: Vec<NotificationRecord> = history.clone();
        drop(history);

        if let Some(opts) = options {
            if opts.unread_only == Some(true) {
                results.retain(|n| !n.read);
            }
            if let Some(ref t) = opts.r#type {
                results.retain(|n| &n.r#type == t);
            }
            if let Some(ref src) = opts.source {
                results.retain(|n| &n.source == src);
            }
            if let Some(limit) = opts.limit {
                let start = results.len().saturating_sub(limit);
                results = results.split_off(start);
                results.reverse();
            } else {
                results.reverse();
            }
        } else {
            results.reverse();
        }

        results
    }

    /// Return all notification history, unfiltered. Equivalent to `get_history(None)`.
    pub fn get_all_history(&self) -> Vec<NotificationRecord> {
        self.get_history(None)
    }

    /// Mark a notification as read by its ID.
    pub fn mark_read(&self, notification_id: &str) -> Result<bool, NotificationError> {
        let mut history = self.history.write();
        let record = history
            .iter_mut()
            .find(|n| n.id == notification_id)
            .ok_or_else(|| NotificationError::NotFound(notification_id.to_string()))?;

        record.read = true;
        let record_id = notification_id.to_string();
        drop(history);

        self.emit_event(
            "notifications:updated",
            &serde_json::json!({
                "id": record_id,
                "read": true,
            }),
        );

        Ok(true)
    }

    /// Mark all notifications as read.
    pub fn mark_all_read(&self) -> usize {
        let mut history = self.history.write();
        let mut count = 0;
        for n in history.iter_mut() {
            if !n.read {
                n.read = true;
                count += 1;
            }
        }
        count
    }

    /// Dismiss (remove) a notification by its ID.
    pub fn dismiss(&self, notification_id: &str) -> Result<bool, NotificationError> {
        let mut history = self.history.write();
        let idx = history
            .iter()
            .position(|n| n.id == notification_id)
            .ok_or_else(|| NotificationError::NotFound(notification_id.to_string()))?;

        history.remove(idx);
        let record_id = notification_id.to_string();
        drop(history);

        self.emit_event(
            "notifications:dismissed",
            &serde_json::json!({
                "id": record_id,
            }),
        );

        Ok(true)
    }

    /// Clear all notifications.
    pub fn clear_all(&self) -> usize {
        let mut history = self.history.write();
        let count = history.len();
        history.clear();
        count
    }

    /// Get the count of unread notifications.
    pub fn get_unread_count(&self) -> usize {
        let history = self.history.read();
        history.iter().filter(|n| !n.read).count()
    }

    /// Get notification count summary.
    pub fn get_counts(&self) -> NotificationCounts {
        let history = self.history.read();
        let total = history.len();
        let mut unread = 0;
        let mut by_type = NotificationTypeCounts {
            info: 0,
            warning: 0,
            error: 0,
            success: 0,
            needs_input: 0,
            task_complete: 0,
            task_error: 0,
        };
        for n in history.iter() {
            if !n.read {
                unread += 1;
            }
            match n.r#type {
                NotificationType::Info => by_type.info += 1,
                NotificationType::Warning => by_type.warning += 1,
                NotificationType::Error => by_type.error += 1,
                NotificationType::Success => by_type.success += 1,
                NotificationType::NeedsInput => by_type.needs_input += 1,
                NotificationType::TaskComplete => by_type.task_complete += 1,
                NotificationType::TaskError => by_type.task_error += 1,
            }
        }
        NotificationCounts {
            total,
            unread,
            by_type,
        }
    }
}
