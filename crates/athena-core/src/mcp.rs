//! MCP (Model Context Protocol) server module — ported from electron/mcpServer.ts
//!
//! Implements a TCP-based JSON-RPC 2.0 server on port 4545 that exposes
//! Athena's tool interface to external agents and plugins.

use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::tool_executor::{ToolCallResult, ToolExecutor, ToolExecutorError};

#[path = "mcp_protocol.rs"]
mod mcp_protocol;
pub use mcp_protocol::{
    get_tools, AgentCommsHandler, JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpError,
    OutputHandler, SpawnHandler, TaskHandler, ToolDefinition, ToolSchema,
};

#[path = "mcp_dispatch.rs"]
mod mcp_dispatch;

#[path = "mcp_transport.rs"]
mod mcp_transport;

#[cfg(test)]
#[path = "mcp_integration_tests.rs"]
mod mcp_integration_tests;

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
    port: Option<u16>,
    app_shutdown: Arc<AtomicBool>,
    tcp_shutdown: Option<Arc<AtomicBool>>,
    tcp_stopped: Option<Arc<AtomicBool>>,
    pub task_handler: Option<TaskHandler>,
    pub spawn_handler: Option<SpawnHandler>,
    pub output_handler: Option<OutputHandler>,
    pub agent_comms_handler: Option<AgentCommsHandler>,
    /// Optional reference to the tool executor for delegating tool calls
    pub tool_executor: Option<Arc<parking_lot::Mutex<ToolExecutor>>>,
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("token", &"[REDACTED]")
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
        Self::new_with_shutdown(Arc::new(AtomicBool::new(false)))
    }

    /// Construct a server using an externally owned TCP shutdown signal.
    ///
    /// Tauri keeps this signal in `AppState` so synchronous exit callbacks can
    /// cancel MCP even when the async server mutex is contended.
    pub fn new_with_shutdown(app_shutdown: Arc<AtomicBool>) -> Self {
        Self {
            token: uuid::Uuid::new_v4().to_string(),
            active_clients: Arc::new(Mutex::new(HashMap::new())),
            port: None,
            app_shutdown,
            tcp_shutdown: None,
            tcp_stopped: None,
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

    /// Returns the port the server is currently listening on, if initialized.
    pub fn port(&self) -> Option<u16> {
        let stopped = self
            .tcp_stopped
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire));
        let stopping = self
            .tcp_shutdown
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire));
        if stopped || stopping {
            None
        } else {
            self.port
        }
    }

    /// Initialize the MCP TCP server on the given port.
    ///
    /// Binds the listener and spawns a background thread that accepts
    /// connections and dispatches each to its own handler thread.
    ///
    /// Returns `Ok(())` immediately. The server runs in the background.
    /// Calling `init` on the active port is idempotent. A different active
    /// port is rejected rather than silently ignored.
    pub fn init(&mut self, port: u16) -> Result<(), McpError> {
        if self
            .tcp_stopped
            .as_ref()
            .is_some_and(|stopped| stopped.load(Ordering::Acquire))
        {
            self.port = None;
            self.tcp_shutdown = None;
            self.tcp_stopped = None;
        }

        if let Some(active_port) = self.port() {
            if active_port == port {
                log::debug!("MCP server already initialized on port {}", active_port);
                return Ok(());
            }
            return Err(McpError::Generic(format!(
                "MCP server is already listening on port {active_port}; requested {port}"
            )));
        }

        // Each bind gets a fresh generation signal. Never reset an old
        // generation's flag, otherwise a delayed old task could be revived by
        // a later reinitialization.
        if let Some(stopped) = self.tcp_stopped.as_ref() {
            if !stopped.load(Ordering::Acquire) {
                return Err(McpError::Generic(
                    "MCP server is still shutting down".to_string(),
                ));
            }
        }
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let listener = std::net::TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let actual_port = listener.local_addr()?.port();
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|e| McpError::Generic(format!("failed to initialize MCP listener: {e}")))?;
        let tcp_shutdown = Arc::new(AtomicBool::new(false));
        let tcp_stopped = Arc::new(AtomicBool::new(false));
        self.tcp_shutdown = Some(Arc::clone(&tcp_shutdown));
        self.tcp_stopped = Some(Arc::clone(&tcp_stopped));
        self.port = Some(actual_port);
        log::info!("MCP server listening on 127.0.0.1:{}", actual_port);

        let token = self.token.clone();
        let app_shutdown = Arc::clone(&self.app_shutdown);
        let active_clients = Arc::clone(&self.active_clients);
        let task_handler = self.task_handler.clone();
        let spawn_handler = self.spawn_handler.clone();
        let output_handler = self.output_handler.clone();
        let agent_comms_handler = self.agent_comms_handler.clone();
        let tool_executor = self.tool_executor.clone();

        tokio::spawn(async move {
            mcp_transport::accept_loop(
                listener,
                tcp_shutdown,
                tcp_stopped,
                app_shutdown,
                token,
                active_clients,
                task_handler,
                spawn_handler,
                output_handler,
                agent_comms_handler,
                tool_executor,
            )
            .await;
        });

        Ok(())
    }

    /// Request TCP shutdown without requiring mutable access to the server.
    ///
    /// Tauri exit callbacks are synchronous and may observe the async mutex
    /// as temporarily contended. Signaling here ensures the accept loop and
    /// active connection readers still stop even if the follow-up mutex lock
    /// is unavailable during that callback.
    pub fn request_shutdown(&self) {
        if let Some(stop) = self.tcp_shutdown.as_ref() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Wait until the current TCP listener generation has fully exited.
    pub async fn wait_for_tcp_shutdown(&self) -> bool {
        let Some(stopped) = self.tcp_stopped.as_ref().cloned() else {
            return true;
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !stopped.load(Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Initialize the MCP server in stdio mode (reads from stdin, writes to stdout).
    ///
    /// Used when Athena is launched as a subprocess by an MCP client
    /// (Claude Code, OpenCode, etc.) via JSON-RPC over stdio.
    pub fn init_stdio(&self) {
        let token = self.token.clone();
        let task_handler = self.task_handler.clone();
        let spawn_handler = self.spawn_handler.clone();
        let output_handler = self.output_handler.clone();
        let agent_comms_handler = self.agent_comms_handler.clone();
        let tool_executor = self.tool_executor.clone();

        tokio::spawn(async move {
            log::info!("MCP stdio server started");
            let reader = BufReader::new(tokio::io::stdin());
            let writer = tokio::io::stdout();
            if let Err(error) = run_stdio_loop(
                reader,
                writer,
                token,
                task_handler,
                spawn_handler,
                output_handler,
                agent_comms_handler,
                tool_executor,
            )
            .await
            {
                log::error!("MCP stdio: I/O error: {error}");
            }
        });
    }

    /// Shutdown the MCP server.
    ///
    /// Signals the TCP accept loop to drop its listener and clears all active
    /// client connections. The accept loop exits asynchronously.
    pub fn shutdown(&mut self) {
        self.request_shutdown();
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
        handle_request_with_executor(
            &self.token,
            req,
            &self.task_handler,
            &self.spawn_handler,
            &self.output_handler,
            &self.agent_comms_handler,
            self.tool_executor.as_ref(),
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

        // Collect cloned streams briefly while holding the lock, then write outside the lock
        let streams: Vec<(String, std::net::TcpStream)> = {
            if let Ok(clients) = self.active_clients.lock() {
                clients
                    .iter()
                    .filter_map(|(peer, stream)| stream.try_clone().ok().map(|s| (peer.clone(), s)))
                    .collect()
            } else {
                return;
            }
        };

        let mut dead_peers = Vec::new();
        for (peer, mut stream) in streams {
            if stream.write_all(bytes).is_err() {
                dead_peers.push(peer);
            }
        }

        // Remove dead peers
        if let Ok(mut clients) = self.active_clients.lock() {
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

/// Run the line-delimited stdio request loop over arbitrary async streams.
///
/// The production entry point supplies stdin/stdout, while tests can use
/// in-memory duplex streams to verify the real stdio protocol without a
/// subprocess or a global stdin/stdout race.
// The stdio transport intentionally mirrors the TCP handler dependencies while
// keeping this generic loop easy to exercise with in-memory streams.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_stdio_loop<R, W>(
    mut reader: R,
    mut writer: W,
    token: String,
    task_handler: Option<TaskHandler>,
    spawn_handler: Option<SpawnHandler>,
    output_handler: Option<OutputHandler>,
    agent_comms_handler: Option<AgentCommsHandler>,
    tool_executor: Option<Arc<parking_lot::Mutex<ToolExecutor>>>,
) -> std::io::Result<()>
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => return Ok(()),
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
                    Ok(request) => request,
                    Err(error) => {
                        log::warn!("MCP stdio: parse error: {error}");
                        let response = mcp_dispatch::make_parse_error_response(trimmed);
                        writer.write_all((response + "\n").as_bytes()).await?;
                        continue;
                    }
                };

                // Stdio is a trusted child-process boundary, so it does not
                // repeat TCP token authentication. Tool calls still use the
                // same executor-backed router as TCP and Tauri.
                let response = handle_request_with_executor(
                    &token,
                    &req,
                    &task_handler,
                    &spawn_handler,
                    &output_handler,
                    &agent_comms_handler,
                    tool_executor.as_ref(),
                )
                .await;

                if response.id.is_some() {
                    let json = McpServer::serialize_response(&response) + "\n";
                    writer.write_all(json.as_bytes()).await?;
                    writer.flush().await?;
                }
            }
            Err(error) => return Err(error),
        }
    }
}

/// Route one request through the canonical executor-backed path.
///
/// Every transport uses this function so external MCP clients cannot
/// accidentally receive the legacy placeholder handlers while the local
/// Tauri command uses the real [`ToolExecutor`].
pub(super) async fn handle_request_with_executor(
    token: &str,
    req: &JsonRpcRequest,
    task_handler: &Option<TaskHandler>,
    spawn_handler: &Option<SpawnHandler>,
    output_handler: &Option<OutputHandler>,
    agent_comms_handler: &Option<AgentCommsHandler>,
    tool_executor: Option<&Arc<parking_lot::Mutex<ToolExecutor>>>,
) -> JsonRpcResponse {
    if req.method == "tools/list" {
        let tools = if tool_executor.is_some() {
            mcp_protocol::get_tools()
        } else {
            Vec::new()
        };
        return JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: Some(serde_json::json!({ "tools": tools })),
            error: None,
        };
    }

    if req.method == "tools/call" {
        if let Some(tool_executor) = tool_executor {
            let name = req
                .params
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let args = req.params.get("arguments").cloned().unwrap_or_default();

            if is_executor_mcp_tool(name) {
                let executor = Arc::clone(tool_executor);
                let tool_name = name.to_string();
                let result = tokio::task::spawn_blocking(move || {
                    let executor = executor.lock();
                    execute_mcp_tool_call(&executor, &tool_name, &args)
                })
                .await;

                let result = match result {
                    Ok(result) => result,
                    Err(error) => Err(ToolExecutorError::Notification(format!(
                        "tool worker failed: {error}"
                    ))),
                };
                return tool_result_response(req, result);
            }
        }
    }

    mcp_dispatch::handle_request_impl(
        token,
        req,
        task_handler,
        spawn_handler,
        output_handler,
        agent_comms_handler,
    )
    .await
}

fn is_executor_mcp_tool(name: &str) -> bool {
    matches!(
        name,
        // Legacy MCP aliases.
        "create_tasks"
            | "get_next_task"
            | "update_task_status"
            | "spawn_agents"
            | "get_output"
            | "list_agent_panes"
            | "code_search"
            | "search_files"
    ) || crate::tool_schema::orchestrator_tools()
        .iter()
        .any(|tool| tool.name == name)
}

fn execute_mcp_tool_call(
    executor: &ToolExecutor,
    name: &str,
    args: &serde_json::Value,
) -> Result<ToolCallResult, ToolExecutorError> {
    // The legacy `create_tasks` contract accepts a batch. Execute each task
    // through the canonical single-task executor so the external route has
    // the same persistence and validation behavior as local tool calls.
    if name == "create_tasks" {
        if let Some(tasks) = args.get("tasks").and_then(|value| value.as_array()) {
            let space_id = args
                .get("spaceId")
                .or_else(|| args.get("space_id"))
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            let mut messages = Vec::with_capacity(tasks.len());
            for task in tasks {
                let mut task_args = task.clone();
                let object = task_args
                    .as_object_mut()
                    .ok_or_else(|| ToolExecutorError::MissingParam("tasks[].title".into()))?;
                if let Some(space_id) = space_id.as_ref() {
                    object
                        .entry("space_id".to_string())
                        .or_insert_with(|| serde_json::Value::String(space_id.clone()));
                }
                let input = mcp_dispatch::args_to_tool_input(&task_args)
                    .ok_or_else(|| ToolExecutorError::MissingParam("tasks[].title".into()))?;
                let result = executor.execute_tool_call("kanban_create_task", &input)?;
                messages.push(result.text);
            }
            return Ok(ToolCallResult {
                text: messages.join("\n"),
                is_error: None,
            });
        }
    }

    let mut normalized = args.clone();
    if let Some(object) = normalized.as_object_mut() {
        // Preserve the historical MCP names while translating them to the
        // canonical executor input fields. `spawn_agents` historically used
        // count/instruction; the executor uses agent_count/task_prompt.
        if name == "spawn_agents" {
            if let Some(value) = object.get("count").cloned() {
                object.entry("agent_count").or_insert(value);
            }
            if let Some(value) = object.get("instruction").cloned() {
                object.entry("task_prompt").or_insert(value);
            }
            object
                .entry("agent_type")
                .or_insert_with(|| serde_json::Value::String("claude".into()));
        }
    }

    let input = mcp_dispatch::args_to_tool_input(&normalized)
        .ok_or_else(|| ToolExecutorError::MissingParam("arguments".into()))?;
    let executor_name = mcp_dispatch::map_mcp_to_executor_name(name);
    executor.execute_tool_call(executor_name, &input)
}

fn tool_result_response(
    req: &JsonRpcRequest,
    result: Result<ToolCallResult, ToolExecutorError>,
) -> JsonRpcResponse {
    let payload = match result {
        Ok(result) => {
            let mut payload = serde_json::json!({
                "content": [{ "type": "text", "text": result.text }]
            });
            if let Some(is_error) = result.is_error {
                payload["isError"] = serde_json::Value::Bool(is_error);
            }
            payload
        }
        Err(error) => serde_json::json!({
            "isError": true,
            "content": [{ "type": "text", "text": error.to_string() }]
        }),
    };
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id: req.id.clone(),
        result: Some(payload),
        error: None,
    }
}

// ---------------------------------------------------------------------------
// TCP accept loop and per-connection handler
// ---------------------------------------------------------------------------
