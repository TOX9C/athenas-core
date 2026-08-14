//! MCP JSON-RPC protocol contracts and canonical tool schemas.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Error type for MCP server operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// A low-level I/O error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The client provided an invalid or missing authentication token.
    #[error("Invalid or missing auth token")]
    InvalidToken,
    /// The requested JSON-RPC method is not recognized.
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    /// The requested MCP tool does not exist.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    /// A mutex lock was poisoned (typically indicates a panic in a holding thread).
    #[error("Lock poisoned")]
    LockPoisoned,
    /// A generic error with a human-readable message.
    #[error("{0}")]
    Generic(String),
}

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request received from a connected client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: String,
    /// Request ID. `None` for notifications (which expect no response).
    pub id: Option<serde_json::Value>,
    /// The method name being invoked (e.g., `"initialize"`, `"tools/call"`).
    pub method: String,
    /// Method-specific parameters.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 response sent back to a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: String,
    /// Echoes the request ID. Absent for notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// The result of a successful request. Mutually exclusive with `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error details if the request failed. Mutually exclusive with `result`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code. `-32600` for invalid request, `-32601` for method not found.
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

/// JSON Schema describing the input parameters for an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Schema type, always `"object"`.
    #[serde(rename = "type")]
    pub type_: String,
    /// Property definitions for the input object.
    pub properties: serde_json::Value,
    /// Names of properties that are required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Definition of a single tool exposed via the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The tool name, used by clients to invoke it (e.g., `"create_tasks"`).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: ToolSchema,
}

/// Build the MCP tools that are executable through the external transport.
///
/// The historical protocol also contained agent-reporting tools that belong to
/// the dedicated agent-comms channel, not this request/response executor. They
/// are intentionally omitted here until they have a real MCP implementation;
/// advertising a tool that returns a placeholder is worse than a smaller,
/// accurate discovery result.
pub fn get_tools() -> Vec<ToolDefinition> {
    let tools = vec![
        ToolDefinition {
            name: "create_tasks".into(),
            description: "Add new tasks to the Kanban board.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "spaceId": { "type": "string" },
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "description": { "type": "string" }
                            },
                            "required": ["title"]
                        }
                    }
                }),
                required: Some(vec!["spaceId".into(), "tasks".into()]),
            },
        },
        ToolDefinition {
            name: "get_next_task".into(),
            description: "Pull the next available To Do task from the board.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({}),
                required: None,
            },
        },
        ToolDefinition {
            name: "update_task_status".into(),
            description: "Update the status of a specific task.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "taskId": { "type": "string" },
                    "status": { "type": "string", "enum": ["todo", "in_progress", "in_review", "complete"] }
                }),
                required: Some(vec!["taskId".into(), "status".into()]),
            },
        },
        ToolDefinition {
            name: "spawn_agents".into(),
            description: "Spawn new terminal worker agents.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "count": { "type": "number" },
                    "instruction": { "type": "string" }
                }),
                required: Some(vec!["count".into()]),
            },
        },
        ToolDefinition {
            name: "notify".into(),
            description: "Send a notification to Athena. Use this to surface important information, warnings, or completion messages to the user.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "level": { "type": "string", "enum": ["info", "warning", "error", "success"], "description": "Notification severity" },
                    "title": { "type": "string", "description": "Short title for the notification" },
                    "message": { "type": "string", "description": "Detailed message body" },
                    "metadata": { "type": "object", "description": "Optional structured metadata" }
                }),
                required: Some(vec!["message".into()]),
            },
        },
        ToolDefinition {
            name: "status_update".into(),
            description: "Update your current working status in Athena. Use this to indicate what you are doing, report progress, or signal that you need input.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "status": { "type": "string", "enum": ["idle", "thinking", "working", "waiting_for_input", "completed", "error", "cancelled"], "description": "Current agent status" },
                    "message": { "type": "string", "description": "Human-readable status description" },
                    "progress": { "type": "object", "properties": { "current": { "type": "number" }, "total": { "type": "number" }, "label": { "type": "string" } }, "description": "Progress indicator" }
                }),
                required: Some(vec!["status".into()]),
            },
        },
        ToolDefinition {
            name: "get_output".into(),
            description: "Read captured terminal output from an agent pane. Returns line-numbered, timestamped output entries.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "paneId": { "type": "string", "description": "The pane ID to read output from." },
                    "limit": { "type": "number", "description": "Maximum number of lines to return. Defaults to 100." },
                    "sinceLine": { "type": "number", "description": "Only return lines with lineNum greater than this value." },
                    "sinceTime": { "type": "number", "description": "Only return lines with timestamp greater than this Unix ms value." }
                }),
                required: Some(vec!["paneId".into()]),
            },
        },
        ToolDefinition {
            name: "list_agent_panes".into(),
            description: "List all agent panes with captured output available.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({}),
                required: None,
            },
        },
        ToolDefinition {
            name: "athena_forward_output".into(),
            description: "Forward agent stdout/stderr output to Athena. Used by plugins to stream terminal output back to the Athena UI.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "entries": { "type": "array", "items": { "type": "object", "properties": { "channel": { "type": "string", "enum": ["stdout", "stderr"] }, "text": { "type": "string" }, "timestamp": { "type": "number" } }, "required": ["channel", "text"] } },
                    "sessionId": { "type": "string" }
                }),
                required: Some(vec!["entries".into()]),
            },
        },
        ToolDefinition {
            name: "send_message_to_agent".into(),
            description: "Send a message to another agent via the agent communications channel.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "target_agent_id": { "type": "string" },
                    "message": { "type": "string" },
                    "message_type": { "type": "string", "enum": ["instruction", "query", "result", "notification"] }
                }),
                required: Some(vec!["target_agent_id".into(), "message".into()]),
            },
        },
        ToolDefinition {
            name: "read_agent_messages".into(),
            description: "List all connected agent sessions.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "agent_id": { "type": "string" }
                }),
                required: None,
            },
        },
        ToolDefinition {
            name: "request_input".into(),
            description: "Request input from the user. Use this when an agent needs clarification or a decision to proceed.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "prompt": { "type": "string", "description": "The question or prompt to present to the user" },
                    "title": { "type": "string", "description": "Optional title for the input request" }
                }),
                required: Some(vec!["prompt".into()]),
            },
        },
        ToolDefinition {
            name: "code_search".into(),
            description: "Search the codebase for a pattern using ripgrep.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "glob": { "type": "string" },
                    "case_sensitive": { "type": "boolean" },
                    "max_results": { "type": "number" },
                    "context_lines": { "type": "number" }
                }),
                required: Some(vec!["pattern".into(), "path".into()]),
            },
        },
        ToolDefinition {
            name: "search_files".into(),
            description: "Search the codebase for a pattern using ripgrep with enhanced edge-case handling.".into(),
            input_schema: ToolSchema {
                type_: "object".into(),
                properties: serde_json::json!({
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "glob": { "type": "string" },
                    "case_sensitive": { "type": "boolean" },
                    "max_results": { "type": "number" },
                    "context_lines": { "type": "number" }
                }),
                required: Some(vec!["pattern".into(), "path".into()]),
            },
        },
    ];

    let mut exposed: Vec<_> = tools
        .into_iter()
        .filter(|tool| {
            matches!(
                tool.name.as_str(),
                "create_tasks"
                    | "get_next_task"
                    | "update_task_status"
                    | "spawn_agents"
                    | "get_output"
                    | "list_agent_panes"
                    | "code_search"
                    | "search_files"
            )
        })
        .collect();

    // Keep the canonical executor schema as the source of truth for the
    // modern names. The legacy aliases above remain for existing clients.
    for tool in crate::tool_schema::orchestrator_tools() {
        let input_schema = tool.input_schema;
        exposed.push(ToolDefinition {
            name: tool.name,
            description: tool.description,
            input_schema: ToolSchema {
                type_: input_schema
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("object")
                    .to_string(),
                properties: input_schema
                    .get("properties")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
                required: input_schema.get("required").and_then(|value| {
                    value.as_array().map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(str::to_string))
                            .collect()
                    })
                }),
            },
        });
    }

    exposed
}

// ---------------------------------------------------------------------------
// Handler callback types
// ---------------------------------------------------------------------------

pub type TaskHandler = Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type SpawnHandler = Arc<dyn Fn(&serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type OutputHandler = Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type AgentCommsHandler =
    Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;
