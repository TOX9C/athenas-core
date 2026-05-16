use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::workspace::AgentType;

/// Capabilities a plugin can advertise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum PluginCapability {
    Notifications,
    Status,
    Tasks,
    AgentControl,
    UserInput,
    FileAccess,
    Swarm,
}

/// Events a plugin may subscribe to or emit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum PluginEventType {
    Notification,
    StatusUpdate,
    TaskComplete,
    TaskError,
    NeedsInput,
    AgentSpawned,
    AgentExited,
    AgentStalled,
    ProgressUpdate,
    ArtifactProduced,
    UserResponse,
    ControlCommand,
    AgentConnected,
    AgentDisconnected,
    PluginRegistered,
    PluginError,
    OutputForwarded,
}

/// Status of a running agent within a pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Thinking,
    Working,
    WaitingForInput,
    Completed,
    Error,
    Cancelled,
    Disconnected,
}

/// Per-pane agent status snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerPaneAgentStatus {
    pub pane_id: String,
    pub status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<super::notification::ProgressInfo>,
    pub last_updated_at: i64,
}

/// Source metadata for a plugin event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEventSource {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub agent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Artifact type produced by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum ArtifactType {
    File,
    Url,
    Image,
    Log,
}

/// An artifact produced during plugin execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    #[serde(rename = "type")]
    pub artifact_type: ArtifactType,
}

/// Level for a plugin event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum PayloadLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Status variant carried in a plugin event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum PayloadStatus {
    Idle,
    Thinking,
    Working,
    WaitingForInput,
    Completed,
    Error,
    Cancelled,
}

/// A control command sent to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ControlCommand {
    Pause,
    Resume,
    Cancel,
}

/// Response type for a user input event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ResponseType {
    Option,
    Freetext,
}

/// Output channel for forwarded entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum OutputChannel {
    Stdout,
    Stderr,
}

/// A forwarded output entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputEntry {
    pub channel: OutputChannel,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

/// The rich payload of a plugin event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEventPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<PayloadLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PayloadStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<super::notification::ProgressInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_type: Option<ResponseType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<ControlCommand>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<OutputEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<PluginCapability>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<super::notification::NotificationPriority>,
}

/// An event emitted or received by a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: PluginEventType,
    pub source: PluginEventSource,
    pub payload: PluginEventPayload,
    pub timestamp: i64,
}

/// Phase of a plugin tool (1 = core, 2 = extended, 3 = experimental).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginToolPhase(pub u8);

/// Definition of a tool exposed by a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub capability: PluginCapability,
    pub phase: PluginToolPhase,
}

/// Schema and defaults for plugin configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginConfigSchema {
    pub schema: serde_json::Value,
    pub defaults: serde_json::Value,
}

/// How a plugin is installed and invoked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginInstallMethod {
    Builtin,
    McpServer {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    Hook {
        script: String,
    },
}

/// Full manifest of a plugin, read from its package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub min_athena_version: String,
    pub capabilities: Vec<PluginCapability>,
    pub tools: Vec<PluginToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribes_to: Option<Vec<PluginEventType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PluginConfigSchema>,
    pub install: PluginInstallMethod,
}

/// Runtime status of an installed plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum PluginStatus {
    Active,
    Inactive,
    Error,
    Installing,
    Updating,
}

/// A currently installed plugin record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub status: PluginStatus,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    pub installed_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub agent_count: usize,
    pub capabilities: Vec<PluginCapability>,
}

/// Default capabilities per agent type.
pub fn default_capabilities() -> HashMap<AgentType, Vec<PluginCapability>> {
    let mut map = HashMap::new();
    map.insert(
        AgentType::Claude,
        vec![
            PluginCapability::Notifications,
            PluginCapability::Status,
            PluginCapability::Tasks,
            PluginCapability::UserInput,
        ],
    );
    map.insert(
        AgentType::Codex,
        vec![
            PluginCapability::Notifications,
            PluginCapability::Status,
            PluginCapability::Tasks,
            PluginCapability::UserInput,
        ],
    );
    map.insert(
        AgentType::Opencode,
        vec![
            PluginCapability::Notifications,
            PluginCapability::Status,
            PluginCapability::Tasks,
            PluginCapability::UserInput,
        ],
    );
    map.insert(
        AgentType::Gemini,
        vec![
            PluginCapability::Notifications,
            PluginCapability::Status,
            PluginCapability::Tasks,
            PluginCapability::UserInput,
        ],
    );
    map.insert(
        AgentType::Custom,
        vec![PluginCapability::Notifications, PluginCapability::Status],
    );
    map.insert(
        AgentType::Shell,
        vec![PluginCapability::Notifications, PluginCapability::Status],
    );
    map
}
