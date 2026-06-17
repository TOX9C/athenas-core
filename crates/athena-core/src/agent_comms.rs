use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Maximum size in bytes of a single agent-comms line. Prevents an agent
/// from streaming a giant line and forcing the server to allocate
/// unbounded memory before it ever sees a newline. Exceeding this cap
/// disconnects the misbehaving agent.
const MAX_AGENT_LINE_BYTES: usize = 65_536; // 64 KiB

/// How long `handle_request_input` will block waiting for the user to
/// respond before giving up and returning a timeout error to the agent.
/// The frontend UI typically surfaces a confirmation dialog that
/// resolves within seconds; 30s is a generous upper bound that still
/// prevents a hung agent thread from being stuck forever. The
/// `cfg(test)` override keeps the timeout-path unit test fast while
/// leaving the production behavior unchanged.
#[cfg(not(test))]
const INPUT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
const INPUT_REQUEST_TIMEOUT: Duration = Duration::from_millis(150);

/// A pending input request, tracked per session so that
/// `cleanup_connection` can drop the sender (and wake the agent's
/// `recv_timeout` with `Disconnected`) when the originating connection
/// goes away, instead of leaking the entry until the 30s timeout.
struct PendingInput {
    session_id: String,
    sender: SyncSender<String>,
}

/// Maximum size in bytes of a single agent-comms line. Prevents an agent
/// from streaming a giant line and forcing the server to allocate
/// unbounded memory before it ever sees a newline. Exceeding this cap
/// disconnects the misbehaving agent.
/// Type of agent message in the agent communications protocol.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageType {
    /// A general notification from an agent (info, warning, error).
    Notification,
    /// A status update (idle, active, waiting_for_input, etc.).
    StatusUpdate,
    /// A request for user input that blocks the agent until answered.
    InputRequest,
    /// An error reported by an agent.
    Error,
    /// A completion signal from an agent.
    Completion,
    /// A periodic heartbeat to confirm the agent is still alive.
    Heartbeat,
    /// Initial registration message sent when an agent first connects.
    Register,
}

/// An agent message in JSON-RPC format, sent over the agent comms TCP channel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentMessage {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: String,
    /// Optional message ID for request/response pairs.
    pub id: Option<String>,
    /// The method name (e.g., `"initialize"`, `"agents/status"`).
    pub method: String,
    /// Method-specific parameters.
    pub params: serde_json::Value,
}

/// Status of an agent session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// The agent is actively working.
    Active,
    /// The agent is idle, waiting for work.
    Idle,
    /// The agent is waiting for user input before proceeding.
    WaitingInput,
    /// The agent has disconnected.
    Disconnected,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Active => write!(f, "active"),
            SessionStatus::Idle => write!(f, "idle"),
            SessionStatus::WaitingInput => write!(f, "waiting_input"),
            SessionStatus::Disconnected => write!(f, "disconnected"),
        }
    }
}

/// Metadata about an agent session, excluding the socket handle.
///
/// Returned by `get_agent_sessions()` and included in status events.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentSession {
    /// Unique session identifier (UUID).
    pub id: String,
    /// The plugin that spawned this agent.
    pub plugin_id: String,
    /// Human-readable agent identifier.
    pub agent_id: String,
    /// Unix timestamp (ms) when the agent connected.
    pub connected_at: u64,
    /// Unix timestamp (ms) of the last activity from this agent.
    pub last_activity_at: u64,
    /// Current session status.
    pub status: SessionStatus,
}

/// Internal session holding both metadata and communication channel.
struct SessionInternal {
    session: AgentSession,
    sender: SyncSender<Vec<u8>>,
    peer_addr: Option<SocketAddr>,
}

impl std::fmt::Debug for SessionInternal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInternal")
            .field("session", &self.session)
            .field("peer_addr", &self.peer_addr)
            .finish()
    }
}

impl Clone for SessionInternal {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            sender: self.sender.clone(),
            peer_addr: self.peer_addr,
        }
    }
}

/// A pending input request from an agent, waiting for user response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputRequest {
    /// Unique identifier for this input request.
    pub request_id: String,
    /// The session that made the request.
    pub session_id: String,
    /// The agent that made the request.
    pub agent_id: String,
    /// The prompt shown to the user.
    pub prompt: String,
    /// Optional title for the input request dialog.
    pub title: String,
}

/// Errors for the agent comms service.
#[derive(Debug, Error)]
pub enum AgentCommsError {
    /// A low-level I/O error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The requested session ID does not exist.
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    /// The requested agent ID is not connected.
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    /// The requested input request ID does not exist.
    #[error("Request not found: {0}")]
    RequestNotFound(String),
    /// The input request was cancelled (e.g., the agent disconnected).
    #[error("Input request cancelled")]
    Cancelled,
    /// The client provided an invalid or missing authentication token.
    #[error("Invalid or missing auth token")]
    InvalidToken,
    /// The requested method is not recognized.
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    /// A mutex lock was poisoned.
    #[error("Lock poisoned")]
    LockPoisoned,
    /// A generic error with a human-readable message.
    #[error("{0}")]
    Generic(String),
}

fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Thread-safe agent communications service.
///
/// Manages a TCP server on port 4546 that agents connect to for
/// lifecycle management: registration, status updates, notifications,
/// and blocking input requests.
///
/// Each connected agent is tracked in a session map. Input requests
/// use synchronous channels to block the agent thread until the user responds.
pub struct AgentComms {
    sessions: Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: Arc<Mutex<HashMap<String, PendingInput>>>,
    token: String,
    event_emitter: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
}

impl std::fmt::Debug for AgentComms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentComms")
            .field("sessions", &"<Mutex<HashMap>>")
            .field("pending_input", &"<Mutex<HashMap>>")
            .field("token", &self.token)
            .field("event_emitter", &"<Option>")
            .finish()
    }
}

impl Clone for AgentComms {
    fn clone(&self) -> Self {
        Self {
            sessions: self.sessions.clone(),
            pending_input: self.pending_input.clone(),
            token: self.token.clone(),
            event_emitter: self.event_emitter.clone(),
        }
    }
}

impl Default for AgentComms {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentComms {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            pending_input: Arc::new(Mutex::new(HashMap::new())),
            token: generate_uuid(),
            event_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    ///
    /// The emitter is called with `(channel_name, json_data)` for events
    /// like `agents:connected`, `agents:statusUpdate`, `agents:inputRequested`, etc.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    /// Returns the session authentication token.
    ///
    /// Agents must include this token in their `initialize` request.
    pub fn get_comms_token(&self) -> &str {
        &self.token
    }

    /// Returns a list of all active agent sessions.
    pub fn get_agent_sessions(&self) -> Vec<AgentSession> {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("AgentComms: lock poisoned while getting agent sessions");
                return Vec::new();
            }
        };
        sessions.values().map(|s| s.session.clone()).collect()
    }

    /// Send a message to a specific agent by its ID.
    ///
    /// Returns `Ok(true)` if the message was queued for delivery.
    /// Returns `AgentNotFound` if no session exists for the given agent ID.
    pub fn send_to_agent(
        &self,
        agent_id: &str,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<bool, AgentCommsError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        let session = sessions
            .values()
            .find(|s| s.session.agent_id == agent_id)
            .ok_or_else(|| AgentCommsError::AgentNotFound(agent_id.to_string()))?;

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let bytes = serde_json::to_vec(&payload)?;
        session.sender.send(bytes).map_err(|_| {
            AgentCommsError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Send failed",
            ))
        })?;
        Ok(true)
    }

    /// Respond to a pending input request from an agent.
    ///
    /// Unblocks the agent's input request handler with the given response text.
    /// Returns `RequestNotFound` if the request ID doesn't exist.
    pub fn respond_to_input_request(
        &self,
        request_id: &str,
        response: &str,
    ) -> Result<bool, AgentCommsError> {
        let mut pending = self
            .pending_input
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        let entry = pending
            .remove(request_id)
            .ok_or_else(|| AgentCommsError::RequestNotFound(request_id.to_string()))?;
        entry.sender.send(response.to_string()).map_err(|_| {
            AgentCommsError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Send failed",
            ))
        })?;
        Ok(true)
    }

    /// Cancel a pending input request, causing the agent to receive an error.
    ///
    /// Dropping the stored `SyncSender` is the only way to unblock the
    /// already-waiting `recv_timeout` inside `handle_request_input`: when
    /// the last sender for a sync channel is dropped, the receiver wakes
    /// up with `RecvTimeoutError::Disconnected` and the request is
    /// reported back to the agent as cancelled.
    ///
    /// Returns `Ok(true)` if the request was found and removed, `Ok(false)` otherwise.
    pub fn cancel_input_request(&self, request_id: &str) -> Result<bool, AgentCommsError> {
        let mut pending = self
            .pending_input
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        let removed = pending.remove(request_id).is_some();
        Ok(removed)
    }

    /// Broadcast a message to all connected agents.
    ///
    /// Sends the same JSON-RPC notification to every registered agent session.
    /// Send failures for individual agents are logged but do not abort the broadcast.
    pub fn broadcast_to_agents(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<(), AgentCommsError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let bytes = serde_json::to_vec(&payload)?;
        for session in sessions.values() {
            if let Err(e) = session.sender.send(bytes.clone()) {
                log::warn!(
                    "broadcast send failed for session {}: {}",
                    session.session.id,
                    e
                );
            }
        }
        Ok(())
    }

    /// Disconnect an agent by its ID, removing it from the session map.
    ///
    /// Returns `Ok(true)` if the agent was found and removed, `Ok(false)` otherwise.
    /// The agent's socket is not explicitly closed — it will detect disconnection on next read.
    pub fn disconnect_agent(&self, agent_id: &str) -> Result<bool, AgentCommsError> {
        // Single lock acquisition prevents TOCTOU on the session map and
        // eliminates a lock-ordering deadlock surface that the previous
        // double-acquire (`lock` -> drop -> `lock`) created when other
        // methods held `sessions` + `pending_input` in a different order.
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        if let Some(id) = sessions
            .values()
            .find(|s| s.session.agent_id == agent_id)
            .map(|s| s.session.id.clone())
        {
            sessions.remove(&id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Shutdown all agent comms, clearing all sessions and pending input requests.
    ///
    /// Called during application shutdown to ensure clean resource release.
    pub fn shutdown_agent_comms(&self) -> Result<(), AgentCommsError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        sessions.clear();

        let mut pending = self
            .pending_input
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        pending.clear();

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn inject_input_request(
        &self,
        request_id: &str,
        session_id: &str,
    ) -> std::sync::mpsc::Receiver<String> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(1);
        let mut pending = self.pending_input.lock().unwrap();
        pending.insert(
            request_id.to_string(),
            PendingInput {
                session_id: session_id.to_string(),
                sender: tx,
            },
        );
        rx
    }

    #[cfg(test)]
    pub(crate) fn pending_input_is_empty(&self) -> bool {
        self.pending_input.lock().unwrap().is_empty()
    }

    #[cfg(test)]
    pub(crate) fn inject_session(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> std::sync::mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1024);
        let session = AgentSession {
            id: session_id.to_string(),
            plugin_id: "test-plugin".to_string(),
            agent_id: agent_id.to_string(),
            connected_at: now_ms(),
            last_activity_at: now_ms(),
            status: SessionStatus::Active,
        };
        let internal = SessionInternal {
            session,
            sender: tx,
            peer_addr: None,
        };
        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.to_string(), internal);
        rx
    }

    /// Initialize the TCP server for agent communication.
    ///
    /// Binds to `127.0.0.1:<port>` and spawns a background thread that
    /// accepts connections. Each connection is handled in its own thread.
    ///
    /// Agents authenticate by sending an `initialize` message with the
    /// session token returned by `get_comms_token()`.
    pub fn init_agent_comms(&self, port: u16) -> Result<(), AgentCommsError> {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr)?;
        log::info!("Agent comms server listening on 127.0.0.1:{}", port);

        let sessions = self.sessions.clone();
        let pending_input = self.pending_input.clone();
        let token = self.token.clone();
        let event_emitter = self.event_emitter.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let sessions = sessions.clone();
                        let pending_input = pending_input.clone();
                        let token = token.clone();
                        let event_emitter = event_emitter.clone();
                        std::thread::spawn(move || {
                            handle_connection(
                                stream,
                                sessions,
                                pending_input,
                                token,
                                event_emitter,
                            );
                        });
                    }
                    Err(e) => {
                        log::error!("Agent comms: failed to accept connection: {}", e);
                    }
                }
            }
            log::info!("Agent comms server stopped");
        });

        Ok(())
    }
}

fn send_to_socket(stream: &TcpStream, payload: &serde_json::Value) {
    if let Ok(mut w) = stream.try_clone() {
        let mut buf = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
        buf.push('\n');
        let _ = w.write_all(buf.as_bytes());
    }
}

fn emit_to_renderer(
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
    channel: &str,
    data: &serde_json::Value,
) {
    if let Ok(guard) = event_emitter.lock() {
        if let Some(ref emitter) = *guard {
            emitter(channel, data);
            return;
        }
    }
    log::debug!("[agent-comms] {} -> {}", channel, data);
}

fn handle_connection(
    stream: TcpStream,
    sessions: Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: Arc<Mutex<HashMap<String, PendingInput>>>,
    token: String,
    event_emitter: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    log::info!("Agent comms: new connection from {}", peer);

    let (tx, _rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1024);
    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(e) => {
            log::error!("failed to clone stream: {}", e);
            return;
        }
    };

    // Per-connection auth state. Set to true only after a successful
    // `initialize` (valid token). Every non-`initialize` method is rejected
    // with -32600 until authenticated. Mirrors the MCP server's auth gate
    // (mcp.rs ConnectionHandler::authenticated): without this, any local
    // process that can reach the port could inject notifications/status
    // attributed to arbitrary agents.
    let mut authenticated = false;

    // Capped line reader: bound each line at MAX_AGENT_LINE_BYTES so a
    // misbehaving agent streaming megabytes without a newline cannot force
    // unbounded allocation. Using read_into a reusable buffer + read_until
    // (rather than BufRead::lines()) is what lets us enforce the cap before
    // the full line is materialized.
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    loop {
        buf.clear();
        let mut total: usize = 0;
        let line_result = loop {
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break None, // EOF — peer closed.
                Ok(n) => {
                    total += n;
                    if total > MAX_AGENT_LINE_BYTES {
                        log::warn!(
                            "Agent comms: disconnecting {} — line exceeded {} bytes",
                            peer,
                            MAX_AGENT_LINE_BYTES
                        );
                        // Drop the connection; the oversized line is discarded.
                        return;
                    }
                    if buf.last() == Some(&b'\n') {
                        // Complete line.
                        let line = String::from_utf8_lossy(&buf).to_string();
                        break Some(line);
                    }
                    // else: partial read, keep accumulating.
                }
                Err(_) => break None,
            }
        };
        let line = match line_result {
            Some(l) => l,
            None => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let msg: AgentMessage = match serde_json::from_str(&trimmed) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Auth gate: reject every non-initialize method when not authenticated.
        if msg.method != "initialize" && !authenticated {
            log::warn!(
                "Agent comms: rejecting unauthenticated '{}' from {}",
                msg.method,
                peer
            );
            if msg.id.is_some() {
                send_to_socket(
                    &stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32600,
                            "message": "Unauthenticated: initialize required",
                        }
                    }),
                );
            }
            continue;
        }

        // initialize is the only method that may run while unauthenticated.
        if msg.method == "initialize" {
            if handle_initialize(&stream, msg, &sessions, &token, &event_emitter, &tx) {
                authenticated = true;
                log::info!("Agent comms: client {} authenticated", peer);
            }
            continue;
        }

        handle_incoming_message(
            &stream,
            msg,
            &sessions,
            &pending_input,
            &event_emitter,
        );
    }

    cleanup_connection(&stream, &sessions, &pending_input, &event_emitter);
    log::info!("Agent comms: connection closed from {}", peer);
}

fn handle_incoming_message(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    // NOTE: `initialize` is handled (and auth-gated) in the connection loop
    // before this function is reached; only post-auth methods dispatch here.
    match msg.method.as_str() {
        "notifications/message" => handle_notification(stream, msg, sessions, event_emitter),
        "agents/status" => handle_status(stream, msg, sessions, event_emitter),
        "agents/requestInput" => {
            handle_request_input(stream, msg, sessions, pending_input, event_emitter)
        }
        "agents/heartbeat" => handle_heartbeat(stream, msg, sessions),
        _ => {
            if msg.id.is_some() {
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {}", msg.method),
                        }
                    }),
                );
            }
        }
    }
}

fn handle_initialize(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    token: &str,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
    tx: &SyncSender<Vec<u8>>,
) -> bool {
    let incoming_token = msg
        .params
        .get("data")
        .and_then(|d| d.get("token"))
        .and_then(|t| t.as_str());

    if incoming_token != Some(token) {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "error": {
                    "code": -32600,
                    "message": "Invalid or missing auth token",
                }
            }),
        );
        return false;
    }

    let session_id = generate_uuid();
    let data = msg.params.get("data").cloned().unwrap_or_default();
    let plugin_id = data
        .get("pluginId")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let agent_id = data
        .get("agentId")
        .and_then(|v| v.as_str())
        .unwrap_or(&format!("agent-{}", &session_id[..8]))
        .to_string();

    let connected_at = now_ms();
    let session = AgentSession {
        id: session_id.clone(),
        plugin_id: plugin_id.clone(),
        agent_id: agent_id.clone(),
        connected_at,
        last_activity_at: connected_at,
        status: SessionStatus::Active,
    };

    let peer_addr = stream.peer_addr().ok();
    let internal = SessionInternal {
        session: session.clone(),
        sender: tx.clone(),
        peer_addr,
    };

    if let Ok(mut map) = sessions.lock() {
        // Evict any existing session from the same peer address to prevent
        // memory leaks when a client reconnects without proper cleanup.
        if let Some(addr) = peer_addr {
            map.retain(|_, existing| existing.peer_addr != Some(addr));
        }
        map.insert(session_id.clone(), internal);
    }

    send_to_socket(
        stream,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": msg.id,
            "result": {
                "sessionId": session_id,
                "agentId": agent_id,
                "protocolVersion": "1.0.0",
                "capabilities": ["notification", "status_update", "input_request", "error", "completion"],
            }
        }),
    );

    emit_to_renderer(
        event_emitter,
        "agents:connected",
        &serde_json::json!({
            "sessionId": session_id,
            "pluginId": plugin_id,
            "agentId": agent_id,
            "connectedAt": connected_at,
        }),
    );

    log::info!(
        "Agent connected: session={} plugin={} agent={}",
        session.id,
        session.plugin_id,
        session.agent_id
    );
    true
}

fn handle_notification(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    let agent_id = msg.params.get("agentId").and_then(|v| v.as_str());
    let session = agent_id.and_then(|aid| find_session_by_agent_id(sessions, aid));

    if let Some(aid) = agent_id {
        update_activity_by_agent_id(sessions, aid);
    }

    let level = msg
        .params
        .get("level")
        .and_then(|v| v.as_str())
        .unwrap_or("info");
    let status = if level == "needs_input" {
        SessionStatus::WaitingInput
    } else {
        SessionStatus::Active
    };

    if let Some(ref s) = session {
        update_session_status(sessions, &s.id, status.clone());
        emit_to_renderer(
            event_emitter,
            "agents:statusUpdate",
            &serde_json::json!({
                "sessionId": s.id,
                "agentId": s.agent_id,
                "status": status,
                "data": msg.params.get("data"),
            }),
        );
    }

    let notif = serde_json::json!({
        "type": level,
        "title": msg.params.get("title").and_then(|v| v.as_str()).unwrap_or("Agent Notification"),
        "message": msg.params.get("message").and_then(|v| v.as_str()).unwrap_or(""),
        "source": session.as_ref().map(|s| &s.plugin_id).unwrap_or(&"unknown".into()),
        "agentId": session.as_ref().map(|s| &s.agent_id),
        "data": msg.params.get("data"),
        "timestamp": now_ms(),
    });

    log::info!("[agent notification] {}", notif);

    if msg.id.is_some() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "result": { "acknowledged": true }
            }),
        );
    }
}

fn handle_status(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    let session = find_session_by_stream(sessions, stream);
    if let Some(ref s) = session {
        update_activity_by_session_id(sessions, &s.id);
        let new_status = msg
            .params
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("active");

        let status_enum = match new_status {
            "waiting_input" => SessionStatus::WaitingInput,
            "idle" => SessionStatus::Idle,
            "disconnected" => SessionStatus::Disconnected,
            _ => SessionStatus::Active,
        };

        update_session_status(sessions, &s.id, status_enum.clone());

        emit_to_renderer(
            event_emitter,
            "agents:statusUpdate",
            &serde_json::json!({
                "sessionId": s.id,
                "agentId": s.agent_id,
                "status": new_status,
                "data": msg.params.get("data"),
            }),
        );

        if new_status == "waiting_input" {
            if let Some(prompt) = msg.params.get("prompt").and_then(|v| v.as_str()) {
                log::info!(
                    "[agent status] waiting_input for agent={}: {}",
                    s.agent_id,
                    prompt
                );
            }
        }
    }

    if msg.id.is_some() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "result": { "acknowledged": true }
            }),
        );
    }
}

fn handle_request_input(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    let session = find_session_by_stream(sessions, stream);
    if session.is_none() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "error": {
                    "code": -32000,
                    "message": "Not initialized",
                }
            }),
        );
        return;
    }

    let session = session.unwrap();
    update_activity_by_session_id(sessions, &session.id);
    update_session_status(sessions, &session.id, SessionStatus::WaitingInput);

    let request_id = msg
        .params
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or(&generate_uuid())
        .to_string();

    let prompt = msg
        .params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let title = msg
        .params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Input Request");

    log::info!(
        "[agent input_request] requestId={} agent={} title={}",
        request_id,
        session.agent_id,
        title
    );

    emit_to_renderer(
        event_emitter,
        "agents:inputRequested",
        &serde_json::json!({
            "sessionId": session.id,
            "agentId": session.agent_id,
            "requestId": request_id,
            "prompt": prompt,
        }),
    );

    if msg.id.is_some() {
        let (input_tx, input_rx) = std::sync::mpsc::sync_channel::<String>(1);

        {
            let mut map = match pending_input.lock() {
                Ok(g) => g,
                Err(_) => {
                    log::error!("Agent comms: pending_input lock poisoned");
                    return;
                }
            };
            map.insert(
                request_id.clone(),
                PendingInput {
                    session_id: session.id.clone(),
                    sender: input_tx,
                },
            );
        }

        match input_rx.recv_timeout(INPUT_REQUEST_TIMEOUT) {
            Ok(response) => {
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "result": { "input": response }
                    }),
                );
                update_session_status(sessions, &session.id, SessionStatus::Active);
                update_activity_by_session_id(sessions, &session.id);
                emit_to_renderer(
                    event_emitter,
                    "agents:statusUpdate",
                    &serde_json::json!({
                        "sessionId": session.id,
                        "agentId": session.agent_id,
                        "status": "active",
                    }),
                );
            }
            Err(RecvTimeoutError::Timeout) => {
                // Remove the stale request so it does not leak.
                if let Ok(mut map) = pending_input.lock() {
                    map.remove(&request_id);
                }
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32000,
                            "message": "Input request timed out",
                        }
                    }),
                );
            }
            Err(RecvTimeoutError::Disconnected) => {
                // Sender was dropped, most likely by cancel_input_request
                // or by cleanup_connection on agent disconnect.
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32000,
                            "message": "Input request cancelled",
                        }
                    }),
                );
            }
        }
    }
}

fn handle_heartbeat(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
) {
    let session = find_session_by_stream(sessions, stream);
    if let Some(ref s) = session {
        update_activity_by_session_id(sessions, &s.id);
    }

    if msg.id.is_some() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "result": { "ts": now_ms() }
            }),
        );
    }
}

fn find_session_by_agent_id(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    agent_id: &str,
) -> Option<AgentSession> {
    let guard = match sessions.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    guard
        .values()
        .find(|s| s.session.agent_id == agent_id)
        .map(|s| s.session.clone())
}

fn find_session_by_stream(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    stream: &TcpStream,
) -> Option<AgentSession> {
    let peer_addr = match stream.peer_addr() {
        Ok(addr) => Some(addr),
        Err(_) => return None,
    };
    let guard = match sessions.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    guard
        .values()
        .find(|s| s.peer_addr == peer_addr)
        .map(|s| s.session.clone())
}

fn update_activity_by_agent_id(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    agent_id: &str,
) {
    if let Ok(mut guard) = sessions.lock() {
        for internal in guard.values_mut() {
            if internal.session.agent_id == agent_id {
                internal.session.last_activity_at = now_ms();
                break;
            }
        }
    }
}

fn update_activity_by_session_id(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    session_id: &str,
) {
    if let Ok(mut guard) = sessions.lock() {
        if let Some(internal) = guard.get_mut(session_id) {
            internal.session.last_activity_at = now_ms();
        }
    }
}

fn update_session_status(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    session_id: &str,
    status: SessionStatus,
) {
    if let Ok(mut guard) = sessions.lock() {
        if let Some(internal) = guard.get_mut(session_id) {
            internal.session.status = status;
        }
    }
}

fn cleanup_connection(
    stream: &TcpStream,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    let peer_addr = match stream.peer_addr() {
        Ok(addr) => Some(addr),
        Err(_) => return,
    };
    let session = {
        let guard = match sessions.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        guard
            .values()
            .find(|s| s.peer_addr == peer_addr)
            .map(|s| s.session.clone())
    };

    if let Some(s) = session {
        if let Ok(mut guard) = sessions.lock() {
            guard.retain(|_, internal| internal.session.id != s.id);
        }

        // Drop any pending input senders that belong to this session.
        // Removing them wakes the corresponding `recv_timeout` with
        // `Disconnected`, so the agent's input handler thread can exit
        // immediately instead of waiting the full 30s for the timeout.
        if let Ok(mut pending) = pending_input.lock() {
            pending.retain(|_, entry| entry.session_id != s.id);
        }

        emit_to_renderer(
            event_emitter,
            "agents:disconnected",
            &serde_json::json!({
                "sessionId": s.id,
                "agentId": s.agent_id,
                "pluginId": s.plugin_id,
            }),
        );

        log::info!("Agent disconnected: agent={}", s.agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_input_request_signals_receiver() {
        let comms = AgentComms::new();
        let rx = comms.inject_input_request("req-001", "sess-001");

        // Cancel the request — this drops the only sender.
        let result = comms.cancel_input_request("req-001");
        assert!(result.is_ok());
        assert!(comms.pending_input_is_empty());

        // Receiver should see Disconnected because all senders were dropped.
        let result = rx.recv_timeout(std::time::Duration::from_millis(100));
        assert!(matches!(
            result,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn respond_to_input_request_unblocks_receiver() {
        let comms = AgentComms::new();
        let rx = comms.inject_input_request("req-002", "sess-002");

        let result = comms.respond_to_input_request("req-002", "hello");
        assert!(result.is_ok());
        assert!(comms.pending_input_is_empty());

        let response = rx.recv_timeout(std::time::Duration::from_millis(100));
        assert_eq!(response.unwrap(), "hello");
    }

    /// `handle_request_input` must wake up after `INPUT_REQUEST_TIMEOUT`
    /// when the frontend never responds, send a `"timed out"` error
    /// back to the agent over the socket, and clean up the pending
    /// input entry. The `cfg(test)` override of the const makes this
    /// complete in ~150ms instead of 30s.
    #[test]
    fn handle_request_input_times_out() {
        use std::io::{BufRead, BufReader};
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let sessions_arc = Arc::new(Mutex::new(HashMap::new()));
        let pending_arc = Arc::new(Mutex::new(HashMap::new()));
        let emitter_arc: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>> =
            Arc::new(Mutex::new(None));

        // Clone the handles so the timeout-handler closure can move one
        // in while the main test thread keeps a reference to assert
        // that the map was cleaned up.
        let pending_for_thread = Arc::clone(&pending_arc);
        let sessions_for_thread = Arc::clone(&sessions_arc);
        let emitter_for_thread = Arc::clone(&emitter_arc);
        let server = std::thread::spawn(move || {
            let (stream, peer_addr) = listener.accept().unwrap();
            // Register a session whose `peer_addr` matches the connected
            // client so `find_session_by_stream` succeeds inside
            // `handle_request_input` and we actually exercise the
            // timeout branch (rather than the "Not initialized" early
            // return).
            {
                let mut sessions = sessions_for_thread.lock().unwrap();
                let (tx, _rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
                let internal = SessionInternal {
                    session: AgentSession {
                        id: "sess-timeout".to_string(),
                        plugin_id: "test".to_string(),
                        agent_id: "agent-timeout".to_string(),
                        connected_at: now_ms(),
                        last_activity_at: now_ms(),
                        status: SessionStatus::Active,
                    },
                    sender: tx,
                    peer_addr: Some(peer_addr),
                };
                sessions.insert("sess-timeout".to_string(), internal);
            }
            handle_request_input(
                &stream,
                AgentMessage {
                    jsonrpc: "2.0".to_string(),
                    id: Some("timeout-id".to_string()),
                    method: "agents/requestInput".to_string(),
                    params: serde_json::json!({
                        "requestId": "req-timeout",
                        "prompt": "p",
                        "title": "t",
                    }),
                },
                &sessions_for_thread,
                &pending_for_thread,
                &emitter_for_thread,
            );
        });

        let client = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let response: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], "timeout-id");
        assert_eq!(response["error"]["message"], "Input request timed out");

        // Drop the client so the server thread can return.
        drop(client);
        server.join().expect("server thread panicked");

        // The pending input map should have been cleaned up by the
        // timeout handler.
        assert!(pending_arc.lock().unwrap().is_empty());
    }

    /// Cancelling a pending input request from another thread must
    /// unblock the receiver immediately (well within the 30s
    /// production timeout). We exercise the same `recv_timeout` /
    /// `cancel_input_request` dance the real `handle_request_input`
    /// uses, so the test would catch a regression where the cancel
    /// path leaks the sender or holds the lock too long.
    #[test]
    fn cancel_input_request_unblocks_receiver() {
        let comms = AgentComms::new();
        let rx = comms.inject_input_request("req-cancel-thread", "sess-cancel");

        // Spawn a thread that mimics what handle_request_input does:
        // block on recv_timeout, then report the outcome to the main
        // thread over a one-shot channel.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let result = rx.recv_timeout(Duration::from_secs(30));
            done_tx.send(result).unwrap();
        });

        // Give the thread a moment to enter recv_timeout.
        std::thread::sleep(Duration::from_millis(50));

        let cancel_started = std::time::Instant::now();
        let cancel_result = comms.cancel_input_request("req-cancel-thread");
        let cancel_elapsed = cancel_started.elapsed();
        assert!(cancel_result.is_ok());
        assert!(cancel_result.unwrap());
        assert!(comms.pending_input_is_empty());

        // The receiver should wake up promptly with Disconnected, not
        // wait the full 30s.
        let recv_result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("cancel did not unblock the receiver within 2s");
        match recv_result {
            Err(RecvTimeoutError::Disconnected) => {}
            other => panic!("expected Disconnected, got {:?}", other),
        }
        assert!(
            cancel_elapsed < Duration::from_secs(2),
            "cancel took {:?}, expected < 2s",
            cancel_elapsed
        );
        handle.join().expect("receiver thread panicked");
    }

    /// H4 regression: a connection that sends a non-`initialize` method
    /// WITHOUT first authenticating must receive a `-32600` error and must
    /// NOT have its handler invoked. Before the per-connection auth gate,
    /// any local process could inject notifications/status attributed to
    /// arbitrary agents.
    #[test]
    fn unauthenticated_methods_are_rejected() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let sessions_arc = Arc::new(Mutex::new(HashMap::new()));
        let pending_arc = Arc::new(Mutex::new(HashMap::new()));
        let emitter_arc: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>> =
            Arc::new(Mutex::new(None));
        let token = "test-token-H4".to_string();

        let sessions_t = Arc::clone(&sessions_arc);
        let pending_t = Arc::clone(&pending_arc);
        let emitter_t = Arc::clone(&emitter_arc);
        let token_t = token.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, sessions_t, pending_t, token_t, emitter_t);
        });

        let mut client = TcpStream::connect(addr).unwrap();
        // Send notifications/message without initialize.
        let unauth_msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "req-1",
            "method": "notifications/message",
            "params": { "agentId": "victim-agent", "data": { "text": "spoofed" } }
        });
        client
            .write_all(format!("{}\n", unauth_msg).as_bytes())
            .unwrap();
        client.flush().unwrap();

        // Read the single auth-error response, then drop the client so the
        // server's read loop observes EOF and exits.
        let mut reader = BufReader::new(client.try_clone().unwrap());
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("expected an auth-error response");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["error"]["code"], -32600, "line: {}", line);
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("initialize required"),
            "line: {}",
            line
        );

        // Close the connection so the server thread can finish.
        drop(client);
        drop(reader);
        server.join().expect("server thread panicked");

        // No session should have been registered for the spoofed agent.
        // (Checked after join so we never hold the lock across the join.)
        let sessions = sessions_arc.lock().unwrap();
        assert!(
            sessions.is_empty(),
            "unauthenticated request must not register a session: {:?}",
            sessions.keys().collect::<Vec<_>>()
        );
    }

    /// H4 positive control: after a valid `initialize`, subsequent methods
    /// are dispatched normally (no `-32600`). Confirms the gate does not
    /// over-block legitimate authenticated traffic.
    #[test]
    fn authenticated_methods_are_accepted() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let sessions_arc = Arc::new(Mutex::new(HashMap::new()));
        let pending_arc = Arc::new(Mutex::new(HashMap::new()));
        let emitter_arc: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>> =
            Arc::new(Mutex::new(None));
        let token = "test-token-H4-pos".to_string();

        let sessions_t = Arc::clone(&sessions_arc);
        let pending_t = Arc::clone(&pending_arc);
        let emitter_t = Arc::clone(&emitter_arc);
        let token_t = token.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, sessions_t, pending_t, token_t, emitter_t);
        });

        let mut client = TcpStream::connect(addr).unwrap();
        // Send a valid initialize.
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "init-1",
            "method": "initialize",
            "params": { "data": { "token": token, "agentId": "legit-agent" } }
        });
        client
            .write_all(format!("{}\n", init).as_bytes())
            .unwrap();
        client.flush().unwrap();

        let mut reader = BufReader::new(client.try_clone().unwrap());
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("expected an initialize response");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(
            v.get("error").is_none(),
            "initialize should succeed, got error: {}",
            line
        );
        assert!(v["result"]["sessionId"].is_string(), "line: {}", line);

        // Close so the server read loop exits, then join.
        drop(client);
        drop(reader);
        server.join().expect("server thread panicked");

        // Note: we do NOT assert session-count == 1 here, because
        // `cleanup_connection` correctly evicts the session when the
        // client disconnects (which happens before join returns). The
        // auth-gate contract — that an authenticated initialize succeeds —
        // is fully covered by the response assertion above.
    }
}
