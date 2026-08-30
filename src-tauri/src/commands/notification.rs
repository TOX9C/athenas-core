use super::html_escape;
use crate::state::AppState;
use tauri::State;

/// Push a new notification to the notification service.
#[tauri::command]
pub fn notification_push(
    state: State<'_, AppState>,
    title: String,
    message: String,
    level: Option<String>,
) -> Result<String, String> {
    // Sanitize user-supplied notification content to prevent XSS if content
    // is ever rendered in a context that supports HTML or markdown.
    let title = html_escape(title.trim());
    let message = html_escape(message.trim());
    let notif_type = match level.as_deref() {
        Some("warning") => athena_core::notification::NotificationType::Warning,
        Some("error") => athena_core::notification::NotificationType::Error,
        Some("success") => athena_core::notification::NotificationType::Success,
        Some("needs_input") => athena_core::notification::NotificationType::NeedsInput,
        Some("task_complete") => athena_core::notification::NotificationType::TaskComplete,
        Some("task_error") => athena_core::notification::NotificationType::TaskError,
        _ => athena_core::notification::NotificationType::Info,
    };
    let event = athena_core::notification::NotificationEvent {
        r#type: notif_type,
        title,
        message,
        source: "command".to_string(),
        agent_id: None,
        data: None,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        metadata: None,
        actions: None,
        request_id: None,
        event_key: None,
        run_id: None,
        pane_id: None,
        requires_action: false,
    };
    let record = state.notification_service.push_notification(event);
    serde_json::to_string(&record).map_err(|e| e.to_string())
}

/// Get the notification history with optional filtering.
#[tauri::command]
pub fn notification_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<String, String> {
    let options = athena_core::notification::HistoryOptions {
        limit,
        unread_only: None,
        r#type: None,
        source: None,
    };
    let history = state.notification_service.get_history(Some(&options));
    serde_json::to_string(&history).map_err(|e| e.to_string())
}

/// Get the count of unread notifications.
#[tauri::command]
pub fn notification_count(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.notification_service.get_unread_count())
}

/// Mark a specific notification as read.
#[tauri::command]
pub fn notification_mark_read(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .mark_read(&notification_id)
        .map_err(|e| e.to_string())
}

/// Mark all notifications as read. Returns the number of notifications marked.
#[tauri::command]
pub fn notification_mark_all_read(state: State<'_, AppState>) -> usize {
    state.notification_service.mark_all_read()
}

/// Dismiss (remove) a notification from the history.
#[tauri::command]
pub fn notification_dismiss(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .dismiss(&notification_id)
        .map_err(|e| e.to_string())
}

/// Clear all notifications from the history. Returns the number cleared.
#[tauri::command]
pub fn notification_clear_all(state: State<'_, AppState>) -> usize {
    state.notification_service.clear_all()
}

/// Resolve an actionable notification while preserving its history.
#[tauri::command]
pub fn notification_resolve(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<bool, String> {
    state
        .notification_service
        .resolve(&notification_id)
        .map_err(|e| e.to_string())
}

/// Get a breakdown of notification counts by type.
#[tauri::command]
pub fn notification_counts(state: State<'_, AppState>) -> Result<String, String> {
    let counts = state.notification_service.get_counts();
    serde_json::to_string(&counts).map_err(|e| e.to_string())
}
