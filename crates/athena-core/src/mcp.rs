//! MCP (Model Context Protocol) server module — ported from electron/mcpServer.ts
//!
//! Implements a TCP-based JSON-RPC 2.0 server on port 4545 that exposes
//! Athena's tool interface to external agents and plugins.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::tool_executor::ToolExecutor;

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

/// Build the standard set of MCP tools exposed by the Athena orchestrator.
pub fn get_tools() -> Vec<ToolDefinition> {
    vec![
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
                    "cwd": { "type": "string" },
                    "instruction": { "type": "string" }
                }),
                required: Some(vec!["count".into(), "cwd".into()]),
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
    ]
}

// ---------------------------------------------------------------------------
// Handler callback types
// ---------------------------------------------------------------------------

pub type TaskHandler = Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type SpawnHandler = Arc<dyn Fn(&serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type OutputHandler = Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type AgentCommsHandler =
    Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// The MCP (Model Context Protocol) server state.
///
/// Listens on a TCP port for JSON-RPC 2.0 connections from external agents
/// and plugins. Each connection is authenticated via a session token and
/// handled in its own thread.
///
/// # Protocol
/// 1. Client connects to `127.0.0.1:<port>`
/// 2. Sends `initialize` with the session token
/// 3. Server validates token and registers the client for broadcasts
/// 4. Client calls tools via `tools/call`
pub struct McpServer {
    token: String,
    active_clients: Arc<Mutex<HashMap<String, TcpStream>>>,
    listener: Option<TcpListener>,
    port: Option<u16>,
    pub task_handler: Option<TaskHandler>,
    pub spawn_handler: Option<SpawnHandler>,
    pub output_handler: Option<OutputHandler>,
    pub agent_comms_handler: Option<AgentCommsHandler>,
    /// Optional reference to the tool executor for delegating tool calls
    pub tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("token", &self.token)
            .field("port", &self.port)
            .finish()
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            token: uuid::Uuid::new_v4().to_string(),
            active_clients: Arc::new(Mutex::new(HashMap::new())),
            listener: None,
            port: None,
            task_handler: None,
            spawn_handler: None,
            output_handler: None,
            agent_comms_handler: None,
            tool_executor: None,
        }
    }

    /// Returns the session authentication token.
    ///
    /// Clients must include this token in their `initialize` request params.
    pub fn get_token(&self) -> &str {
        &self.token
    }

    /// Initialize the MCP TCP server on the given port.
    ///
    /// Binds the listener and spawns a background thread that accepts
    /// connections and dispatches each to its own handler thread.
    ///
    /// Returns `Ok(())` immediately. The server runs in the background.
    /// Calling `init` on an already-initialized server is a no-op.
    pub fn init(&mut self, port: u16) -> Result<(), McpError> {
        if let Some(port) = self.port {
            log::warn!("MCP server already initialized on port {}", port);
            return Ok(());
        }

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr)?;
        self.port = Some(port);
        log::info!("MCP server listening on 127.0.0.1:{}", port);

        let token = self.token.clone();
        let active_clients = Arc::clone(&self.active_clients);
        let task_handler = self.task_handler.clone();
        let spawn_handler = self.spawn_handler.clone();
        let output_handler = self.output_handler.clone();
        let agent_comms_handler = self.agent_comms_handler.clone();

        tokio::spawn(async move {
            accept_loop(
                listener,
                token,
                active_clients,
                task_handler,
                spawn_handler,
                output_handler,
                agent_comms_handler,
            )
            .await;
        });

        Ok(())
    }

    /// Shutdown the MCP server.
    ///
    /// Drops the TCP listener and clears all active client connections.
    /// The accept loop will terminate on the next iteration.
    pub fn shutdown(&mut self) {
        self.listener = None;
        if let Ok(mut clients) = self.active_clients.lock() {
            clients.clear();
        }
        self.port = None;
        log::info!("MCP server shut down");
    }

    /// Handle an incoming JSON-RPC request.
    ///
    /// Dispatches to the appropriate handler based on the method name.
    /// Returns a `JsonRpcResponse` with either the result or an error.
    pub async fn handle_request(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        // Try to delegate tool calls to the tool executor first
        if req.method == "tools/call" {
            if let Some(ref tool_exec_arc) = self.tool_executor {
                let name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = req
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_default();

                // Map MCP tool name to ToolExecutor tool name
                let executor_name = map_mcp_to_executor_name(name);

                // Convert args to ToolInput
                if let Some(tool_input) = args_to_tool_input(&args) {
                    if let Ok(tool_exec) = tool_exec_arc.lock() {
                        match tool_exec.execute_tool_call(executor_name, &tool_input) {
                            Ok(result) => {
                                return JsonRpcResponse {
                                    jsonrpc: "2.0".into(),
                                    id: req.id.clone(),
                                    result: Some(serde_json::json!({
                                        "content": [{ "type": "text", "text": result.text }]
                                    })),
                                    error: None,
                                };
                            }
                            Err(crate::tool_executor::ToolExecutorError::UnknownTool(_)) => {
                                // Fall through to existing handlers
                            }
                            Err(e) => {
                                return JsonRpcResponse {
                                    jsonrpc: "2.0".into(),
                                    id: req.id.clone(),
                                    result: None,
                                    error: Some(JsonRpcError {
                                        code: -32603,
                                        message: format!("Tool execution error: {}", e),
                                        data: None,
                                    }),
                                };
                            }
                        }
                    }
                }
            }
        }

        // Fall through to the existing handler implementation
        handle_request_impl(
            &self.token,
            req,
            &self.task_handler,
            &self.spawn_handler,
            &self.output_handler,
            &self.agent_comms_handler,
        )
        .await
    }

    /// Broadcast a notification to all connected clients.
    ///
    /// Sends a JSON-RPC notification (no `id` field) with the given method
    /// and params to every registered client. Dead peers are removed.
    pub fn broadcast_notification(&self, method: &str, params: &serde_json::Value) {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()) + "\n";
        let bytes = line.as_bytes();

        if let Ok(mut clients) = self.active_clients.lock() {
            let dead_peers: Vec<String> = clients
                .iter()
                .filter_map(|(peer, stream)| {
                    if let Ok(mut clone) = stream.try_clone() {
                        if clone.write_all(bytes).is_err() {
                            return Some(peer.clone());
                        }
                    }
                    None
                })
                .collect();

            for peer in dead_peers {
                clients.remove(&peer);
            }
        }
    }

    /// Parse a line-delimited JSON-RPC request from a raw string.
    ///
    /// Returns `None` if the string is not valid JSON.
    pub fn parse_request(line: &str) -> Option<JsonRpcRequest> {
        serde_json::from_str(line).ok()
    }

    /// Serialize a JSON-RPC response to a line-delimited JSON string.
    ///
    /// Returns `"{}"` if serialization fails.
    pub fn serialize_response(resp: &JsonRpcResponse) -> String {
        serde_json::to_string(resp).unwrap_or_else(|_| "{}".into())
    }
}

// ---------------------------------------------------------------------------
// Free functions for request handling (shared between McpServer and
// the per-connection handler thread).
// ---------------------------------------------------------------------------

async fn handle_request_impl(
    token: &str,
    req: &JsonRpcRequest,
    task_handler: &Option<TaskHandler>,
    spawn_handler: &Option<SpawnHandler>,
    output_handler: &Option<OutputHandler>,
    agent_comms_handler: &Option<AgentCommsHandler>,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => {
            let params = &req.params;
            if params.get("token").and_then(|t| t.as_str()) != Some(token) {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: req.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid or missing auth token".into(),
                        data: None,
                    }),
                };
            }
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "athena-orchestrator", "version": "1.0.0" }
                })),
                error: None,
            }
        }
        "notifications/initialized" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: Some(serde_json::Value::Null),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: Some(serde_json::json!({ "tools": get_tools() })),
            error: None,
        },
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let result = handle_tool_call_impl(
                name,
                arguments,
                task_handler,
                spawn_handler,
                output_handler,
                agent_comms_handler,
            )
            .await;
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                result: Some(result),
                error: None,
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
                data: None,
            }),
        },
    }
}

async fn handle_tool_call_impl(
    name: &str,
    args: serde_json::Value,
    task_handler: &Option<TaskHandler>,
    spawn_handler: &Option<SpawnHandler>,
    output_handler: &Option<OutputHandler>,
    agent_comms_handler: &Option<AgentCommsHandler>,
) -> serde_json::Value {
    match name {
        "notify" => {
            let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("info");
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Agent Notification");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            log::info!(
                "[MCP notify] level={}, title={}, msg={}",
                level,
                title,
                message
            );
            serde_json::json!({ "content": [{ "type": "text", "text": "Notification delivered." }] })
        }
        "status_update" => {
            let status = args
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("idle");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            log::info!("[MCP status_update] status={}, msg={}", status, message);
            serde_json::json!({ "content": [{ "type": "text", "text": format!("Status updated to: {}", status) }] })
        }
        "request_input" => {
            let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Input Request");
            log::info!("[MCP request_input] title={}, prompt={}", title, prompt);
            serde_json::json!({ "content": [{ "type": "text", "text": "Input request received. (Blocking input not yet available — use environment variables or config files for now.)" }] })
        }
        "create_tasks" => {
            if let Some(handler) = task_handler {
                handler("create_tasks", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'create_tasks' not yet implemented" }] })
            }
        }
        "get_next_task" => {
            if let Some(handler) = task_handler {
                handler("get_next_task", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'get_next_task' not yet implemented" }] })
            }
        }
        "update_task_status" => {
            if let Some(handler) = task_handler {
                handler("update_task_status", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'update_task_status' not yet implemented" }] })
            }
        }
        "spawn_agents" => {
            if let Some(handler) = spawn_handler {
                handler(&args)
            } else {
                let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                serde_json::json!({ "content": [{ "type": "text", "text": format!("Spawn request received for {} agents (placeholder — real implementation requires PTY access)", count) }] })
            }
        }
        "get_output" => {
            if let Some(handler) = output_handler {
                handler("get_output", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'get_output' not yet implemented" }] })
            }
        }
        "list_agent_panes" => {
            if let Some(handler) = output_handler {
                handler("list_agent_panes", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'list_agent_panes' not yet implemented" }] })
            }
        }
        "athena_forward_output" => {
            let entries = args
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let session_id = args.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
            log::info!(
                "[MCP athena_forward_output] entries={}, session={}",
                entries,
                session_id
            );
            serde_json::json!({ "content": [{ "type": "text", "text": format!("Forwarded {} output entries.", entries) }] })
        }
        "send_message_to_agent" => {
            if let Some(handler) = agent_comms_handler {
                handler("send_message_to_agent", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'send_message_to_agent' not yet implemented" }] })
            }
        }
        "read_agent_messages" => {
            if let Some(handler) = agent_comms_handler {
                handler("read_agent_messages", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'read_agent_messages' not yet implemented" }] })
            }
        }
        "code_search" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            let glob = args
                .get("glob")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let context_lines = args
                .get("context_lines")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let options = crate::types::SearchOptions {
                pattern,
                path,
                glob,
                case_sensitive,
                max_results,
                context_lines,
            };

            let search_result = crate::search::search_code(&options).await;

            match search_result {
                Ok(result) => {
                    if result.matches.is_empty() {
                        serde_json::json!({ "content": [{ "type": "text", "text": format!("No matches found for pattern \"{}\" in {}.", options.pattern, options.path) }] })
                    } else {
                        let formatted = result
                            .matches
                            .iter()
                            .map(|m| {
                                let mut output = format!(
                                    "{}:{}:{}: {}",
                                    m.file_path, m.line_number, m.column, m.line_text
                                );
                                if !m.context_before.is_empty() {
                                    let before = m
                                        .context_before
                                        .iter()
                                        .enumerate()
                                        .map(|(i, l)| {
                                            format!(
                                                "  {}: {}",
                                                m.line_number - m.context_before.len() as u32
                                                    + i as u32,
                                                l
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    output = format!("{}\n{}", before, output);
                                }
                                if !m.context_after.is_empty() {
                                    let after = m
                                        .context_after
                                        .iter()
                                        .enumerate()
                                        .map(|(i, l)| {
                                            format!("  {}: {}", m.line_number + 1 + i as u32, l)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    output = format!("{}\n{}", output, after);
                                }
                                output
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n");

                        let header = format!(
                            "Found {} matches in {} files{}:\n\n",
                            result.stats.total_matches,
                            result.stats.files_matched,
                            if result.truncated { " (truncated)" } else { "" }
                        );

                        serde_json::json!({ "content": [{ "type": "text", "text": format!("{}{}", header, formatted) }] })
                    }
                }
                Err(e) => {
                    serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": format!("Search error: {}", e) }] })
                }
            }
        }
        "search_files" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            let glob = args
                .get("glob")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let search_result =
                crate::search::search_files(&path, &pattern, glob.as_deref(), max_results).await;

            match search_result {
                Ok(results) => {
                    if results.is_empty() {
                        serde_json::json!({ "content": [{ "type": "text", "text": format!("No files found matching pattern \"{}\" in {}.", pattern, path) }] })
                    } else {
                        let formatted = results.join("\n");
                        serde_json::json!({ "content": [{ "type": "text", "text": format!("Found {} files:\n\n{}", results.len(), formatted) }] })
                    }
                }
                Err(e) => {
                    serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": format!("Search error: {}", e) }] })
                }
            }
        }
        _ => {
            serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": format!("Unknown tool: {}", name) }] })
        }
    }
}

// ---------------------------------------------------------------------------
// TCP accept loop and per-connection handler
// ---------------------------------------------------------------------------

async fn accept_loop(
    listener: std::net::TcpListener,
    token: String,
    active_clients: Arc<Mutex<HashMap<String, TcpStream>>>,
    task_handler: Option<TaskHandler>,
    spawn_handler: Option<SpawnHandler>,
    output_handler: Option<OutputHandler>,
    agent_comms_handler: Option<AgentCommsHandler>,
) {
    // Convert the std listener to a tokio listener for async accept.
    listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => {
            log::error!("MCP: failed to convert listener to tokio: {}", e);
            return;
        }
    };

    log::info!("MCP accept loop started");
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let handler = ConnectionHandler {
                    token: token.clone(),
                    active_clients: Arc::clone(&active_clients),
                    task_handler: task_handler.clone(),
                    spawn_handler: spawn_handler.clone(),
                    output_handler: output_handler.clone(),
                    agent_comms_handler: agent_comms_handler.clone(),
                };
                tokio::spawn(async move {
                    handler.handle_connection(stream).await;
                });
            }
            Err(e) => {
                log::error!("MCP: failed to accept connection: {}", e);
            }
        }
    }
}

struct ConnectionHandler {
    token: String,
    active_clients: Arc<Mutex<HashMap<String, TcpStream>>>,
    task_handler: Option<TaskHandler>,
    spawn_handler: Option<SpawnHandler>,
    output_handler: Option<OutputHandler>,
    agent_comms_handler: Option<AgentCommsHandler>,
}

impl ConnectionHandler {
    async fn handle_request(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        handle_request_impl(
            &self.token,
            req,
            &self.task_handler,
            &self.spawn_handler,
            &self.output_handler,
            &self.agent_comms_handler,
        )
        .await
    }

    async fn handle_connection(&self, stream: tokio::net::TcpStream) {
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        log::info!("MCP: new connection from {}", peer);

        // Convert back to std briefly to get a clone for active_clients.
        let std_stream = match stream.into_std() {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to convert tokio stream to std: {}", e);
                return;
            }
        };
        let std_clone = match std_stream.try_clone() {
            Ok(c) => c,
            Err(e) => {
                log::error!("failed to clone std stream: {}", e);
                return;
            }
        };
        // Re-convert to tokio for async I/O.
        let stream = match tokio::net::TcpStream::from_std(std_stream) {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to convert std stream back to tokio: {}", e);
                return;
            }
        };

        // Split into read/write halves for non-blocking I/O.
        let (read_half, write_half) = tokio::io::split(stream);
        let reader = BufReader::new(read_half);
        let write_half = Arc::new(tokio::sync::Mutex::new(write_half));
        let mut lines = reader.lines();

        // Register the std TcpStream clone for broadcast_notification
        // (which uses sync I/O). Will be removed on disconnect.
        if let Ok(mut clients) = self.active_clients.lock() {
            clients.insert(peer.clone(), std_clone);
        }

        loop {
            let line = match lines.next_line().await {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => {
                    log::warn!("MCP: read error from {}: {}", peer, e);
                    break;
                }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("MCP: parse error from {}: {}", peer, e);
                    continue;
                }
            };

            let response = self.handle_request(&req).await;

            // Only send a response for requests (notifications have no id)
            if response.id.is_some() {
                let json = McpServer::serialize_response(&response) + "\n";
                let mut writer = write_half.lock().await;
                if writer.write_all(json.as_bytes()).await.is_err() {
                    break;
                }
            }

            // On successful initialize, log it.
            if req.method == "initialize" && response.error.is_none() {
                log::info!("MCP: client {} initialized", peer);
            }

            // On failed initialize, close the connection
            if req.method == "initialize" && response.error.is_some() {
                log::warn!("MCP: rejecting unauthorized client {}", peer);
                break;
            }
        }

        // Remove from active_clients on disconnect
        if let Ok(mut clients) = self.active_clients.lock() {
            clients.remove(&peer);
        }
        log::info!("MCP: connection closed from {}", peer);
    }
}

// ---------------------------------------------------------------------------
// Helper functions for tool executor delegation
// ---------------------------------------------------------------------------

/// Map MCP tool names to ToolExecutor tool names.
fn map_mcp_to_executor_name(mcp_name: &str) -> &str {
    match mcp_name {
        "create_tasks" => "kanban_create_task",
        "get_next_task" => "kanban_list_tasks",
        "update_task_status" => "kanban_update_task",
        "spawn_agents" => "launch_builtin_agent",
        "get_output" => "read_agent_output",
        "list_agent_panes" => "list_agents",
        "code_search" => "fs_search",
        "search_files" => "fs_search",
        "run_command_in_terminals" => "run_command_in_terminals",
        "close_terminals" => "close_terminals",
        "prompt_agent" => "prompt_agent",
        "launch_builtin_agent" => "launch_builtin_agent",
        _ => mcp_name,
    }
}

/// Convert JSON-RPC tool call arguments into a `ToolInput` structure,
/// handling both camelCase and snake_case keys.
fn args_to_tool_input(args: &serde_json::Value) -> Option<crate::tool_executor::ToolInput> {
    let map = args.as_object()?;

    let mut ti = crate::tool_executor::ToolInput::default();

    // Kanban
    ti.title = map.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
    ti.description = map.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
    ti.status = map.get("status").and_then(|v| v.as_str()).map(|s| s.to_string());
    if let Some(v) = map.get("taskId").or_else(|| map.get("task_id")) {
        ti.task_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("spaceId").or_else(|| map.get("space_id")) {
        ti.space_id = v.as_str().map(|s| s.to_string());
    }

    // Agent / Pane
    if let Some(v) = map.get("agentType").or_else(|| map.get("agent_type")) {
        ti.agent_type = v.as_str().map(|s| s.to_string());
    }
    if let Some(n) = map.get("agentCount").or_else(|| map.get("agent_count")).and_then(|v| v.as_u64()) {
        ti.agent_count = Some(n as u32);
    }
    if let Some(v) = map.get("taskPrompt").or_else(|| map.get("task_prompt")) {
        ti.task_prompt = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("command").and_then(|v| v.as_str()) {
        ti.command = Some(v.to_string());
    }
    if let Some(v) = map.get("paneId").or_else(|| map.get("pane_id")) {
        ti.pane_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("agentId").or_else(|| map.get("agent_id")) {
        ti.agent_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(arr) = map.get("paneIds").or_else(|| map.get("pane_ids")) {
        if let Some(arr) = arr.as_array() {
            ti.pane_ids = Some(arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
        }
    }

    // FS / Search
    if let Some(v) = map.get("path").and_then(|v| v.as_str()) {
        ti.path = Some(v.to_string());
    }
    if let Some(v) = map.get("pattern").and_then(|v| v.as_str()) {
        ti.pattern = Some(v.to_string());
    }
    if let Some(n) = map.get("limit").and_then(|v| v.as_u64()) {
        ti.limit = Some(n as usize);
    }
    if let Some(n) = map.get("sinceLine").or_else(|| map.get("since_line")).and_then(|v| v.as_u64()) {
        ti.since_line = Some(n as u32);
    }

    // Plan
    if let Some(v) = map.get("goal").and_then(|v| v.as_str()) {
        ti.goal = Some(v.to_string());
    }
    if let Some(v) = map.get("reasoning").and_then(|v| v.as_str()) {
        ti.reasoning = Some(v.to_string());
    }
    if let Some(v) = map.get("stepId").or_else(|| map.get("step_id")) {
        ti.step_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("planId").or_else(|| map.get("plan_id")) {
        ti.plan_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("prompt").and_then(|v| v.as_str()) {
        ti.prompt = Some(v.to_string());
    }
    if let Some(v) = map.get("overallStatus").or_else(|| map.get("overall_status")) {
        ti.overall_status = v.as_str().map(|s| s.to_string());
    }
    if let Some(arr) = map.get("stepEvaluations").or_else(|| map.get("step_evaluations")) {
        if let Some(arr) = arr.as_array() {
            ti.step_evaluations = Some(arr.clone());
        }
    }
    if let Some(v) = map.get("nextAction").or_else(|| map.get("next_action")) {
        ti.next_action = v.as_str().map(|s| s.to_string());
    }

    // Misc
    if let Some(v) = map.get("question").and_then(|v| v.as_str()) {
        ti.question = Some(v.to_string());
    }
    if let Some(arr) = map.get("options") {
        if let Some(arr) = arr.as_array() {
            ti.options = Some(arr.clone());
        }
    }
    if let Some(v) = map.get("message").and_then(|v| v.as_str()) {
        ti.message = Some(v.to_string());
    }
    if let Some(v) = map.get("targetAgentId").or_else(|| map.get("target_agent_id")) {
        ti.target_agent_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("messageType").or_else(|| map.get("message_type")) {
        ti.message_type = v.as_str().map(|s| s.to_string());
    }

    Some(ti)
}
