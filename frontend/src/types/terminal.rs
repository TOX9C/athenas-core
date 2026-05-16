use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// A captured command block in a terminal session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandBlock {
    pub id: String,
    pub command: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    pub collapsed: bool,
}

/// Status of a PTY session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum PtySessionStatus {
    Idle,
    Running,
    Exited,
    Error,
}

/// A running PTY session with its command history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PtySession {
    pub pane_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub status: PtySessionStatus,
    pub blocks: Vec<CommandBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exit_code: Option<i32>,
}

/// Type of shell integration event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum ShellIntegrationEventType {
    Prompt,
    CommandStart,
    CommandExecuted,
    CommandFinished,
    Cwd,
    Property,
}

/// A shell integration event from the terminal backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellIntegrationEvent {
    #[serde(rename = "type")]
    pub event_type: ShellIntegrationEventType,
    pub pane_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Event: working directory changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellCwdChangedEvent {
    pub pane_id: String,
    pub cwd: String,
    pub timestamp: i64,
}

/// Event: a command started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellCommandStartedEvent {
    pub pane_id: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub timestamp: i64,
}

/// Event: a command exited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellCommandExitedEvent {
    pub pane_id: String,
    pub command: String,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    pub timestamp: i64,
}
