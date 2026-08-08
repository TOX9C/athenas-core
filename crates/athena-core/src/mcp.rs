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

use crate::tool_executor::ToolExecutor;

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

        tokio::spawn(async move {
            log::info!("MCP stdio server started");
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();
            let mut reader = BufReader::new(stdin);
            let writer = Arc::new(tokio::sync::Mutex::new(stdout));
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        log::info!("MCP stdio: EOF on stdin, shutting down");
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
                            Ok(r) => r,
                            Err(e) => {
                                log::warn!("MCP stdio: parse error: {}", e);
                                let err = mcp_dispatch::make_parse_error_response(trimmed);
                                let mut w = writer.lock().await;
                                let _ = w.write_all((err + "\n").as_bytes()).await;
                                continue;
                            }
                        };

                        // For stdio mode, skip authentication since the
                        // process is spawned by the client itself.
                        let response = mcp_dispatch::handle_request_impl(
                            &token,
                            &req,
                            &task_handler,
                            &spawn_handler,
                            &output_handler,
                            &agent_comms_handler,
                        )
                        .await;

                        if response.id.is_some() {
                            let json = McpServer::serialize_response(&response) + "\n";
                            let mut w = writer.lock().await;
                            if w.write_all(json.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("MCP stdio: read error: {}", e);
                        break;
                    }
                }
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
        // Try to delegate tool calls to the tool executor first
        if req.method == "tools/call" {
            if let Some(ref tool_exec_arc) = self.tool_executor {
                let name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = req.params.get("arguments").cloned().unwrap_or_default();

                // Map MCP tool name to ToolExecutor tool name
                let executor_name = mcp_dispatch::map_mcp_to_executor_name(name);

                // Convert args to ToolInput
                if let Some(tool_input) = mcp_dispatch::args_to_tool_input(&args) {
                    let tool_exec = tool_exec_arc.lock();
                    {
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
        mcp_dispatch::handle_request_impl(
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

// ---------------------------------------------------------------------------
// TCP accept loop and per-connection handler
// ---------------------------------------------------------------------------
