//! Public plugin enums and serialized data contracts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enum types
// ---------------------------------------------------------------------------

/// Capabilities a plugin can advertise. Mirrors the TS `PluginCapability`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    Notifications,
    Status,
    Tasks,
    AgentControl,
    UserInput,
    FileAccess,
    Swarm,
}

/// Runtime status of an installed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Installed,
    Enabled,
    Disabled,
    Error,
}

/// Status of a plugin session. Mirrors the TS session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Idle,
    WaitingInput,
    Disconnected,
}

/// Plugin event types. Mirrors the TS `PluginEventType`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Type of AI agent a pane can host.
///
/// Keep in sync with `frontend/src/types/workspace.rs` (same variants, same
/// lowercase serde strings). New agents are detection-driven in v1; plugin
/// capability defaults mirror the agent-like set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    Codex,
    Opencode,
    Gemini,
    Qwen,
    Aider,
    Cursor,
    Freebuff,
    Omp,
    Custom,
    Shell,
}

/// Level for a plugin event payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadLevel {
    Info,
    Warning,
    Error,
    Success,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// How a plugin is installed and invoked. Mirrors the TS `PluginInstallMethod`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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

/// Schema and defaults for plugin configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginConfigSchema {
    pub schema: serde_json::Value,
    pub defaults: serde_json::Value,
}

/// Definition of a tool exposed by a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub capability: PluginCapability,
    pub phase: u8,
}

/// MCP server configuration embedded in a manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// Full manifest of a plugin, typically read from a JSON file on disk.
/// Mirrors the TS `PluginManifest` from `plugin-manager.ts` and `plugin.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<PluginCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_athena_version: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    #[serde(default)]
    pub tools: Vec<PluginToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribes_to: Option<Vec<PluginEventType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PluginConfigSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<PluginInstallMethod>,
}

/// An installed plugin record combining its manifest with runtime state.
/// Mirrors the TS `PluginEntry` from `plugin-manager.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub installed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_enabled_at: Option<i64>,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Public-facing plugin info, safe to expose to the renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub status: PluginStatus,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A plugin session representing a live connection between an agent and a plugin.
/// Mirrors the TS `PluginSession` from `pluginHost.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSession {
    pub id: String,
    pub plugin_id: String,
    pub agent_type: AgentType,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub capabilities: Vec<PluginCapability>,
    pub connected_at: i64,
    pub last_activity_at: i64,
    pub status: SessionStatus,
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
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

/// A full plugin event with generated ID and timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: PluginEventType,
    pub source: PluginEventSource,
    pub payload: PluginEventPayload,
    pub timestamp: i64,
}

/// A pending message awaiting a response from a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub id: String,
    pub session_id: String,
    pub method: String,
    pub params: serde_json::Value,
    pub sent_at: i64,
}

/// Result of a health check pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub checked_at: i64,
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub idle_sessions: usize,
    pub stalled_sessions: usize,
    pub disconnected_sessions: usize,
    pub stalled_session_ids: Vec<String>,
}
