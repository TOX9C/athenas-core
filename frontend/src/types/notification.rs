use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::plugin::AgentStatus;
use super::workspace::AgentType;

/// Kind of notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum NotificationType {
    Info,
    Warning,
    Error,
    Success,
    NeedsInput,
    TaskComplete,
    TaskError,
}

/// Priority level of a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

/// An input request attached to a notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationInputRequest {
    pub request_id: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    pub responding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

/// Style hint for an action button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum NotificationActionStyle {
    Primary,
    Secondary,
    Danger,
}

/// A clickable action on a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
    pub style: NotificationActionStyle,
}

/// A notification displayed in the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    #[serde(rename = "type")]
    pub notification_type: NotificationType,
    pub priority: NotificationPriority,
    pub title: String,
    pub message: String,
    pub timestamp: i64,
    pub read: bool,
    pub dismissed: bool,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<AgentType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_request: Option<NotificationInputRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<NotificationAction>>,
}

/// An entry tracking an agent's live status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatusEntry {
    pub id: String,
    pub name: String,
    pub agent_type: AgentType,
    pub status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    pub last_action: String,
    pub last_action_at: i64,
    pub connected_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressInfo>,
}

/// Progress information for an agent or task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressInfo {
    pub current: usize,
    pub total: usize,
    pub label: String,
}
