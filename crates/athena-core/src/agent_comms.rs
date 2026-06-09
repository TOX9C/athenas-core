use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use thiserror::Error;

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

#[allow(dead_code)]
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
    pending_input: Arc<Mutex<HashMap<String, SyncSender<String>>>>,
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

    #[allow(dead_code)]
    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        if let Ok(guard) = self.event_emitter.lock() {
            if let Some(ref emitter) = *guard {
                emitter(channel, data);
                return;
            }
        }
        log::debug!("[agent-comms] {} -> {}", channel, data);
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

    /// Find a session by agent ID.
    #[allow(dead_code)]
    fn find_session_by_agent_id(&self, agent_id: &str) -> Option<AgentSession> {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("AgentComms: lock poisoned while finding session by agent_id");
                return None;
            }
        };
        sessions
            .values()
            .find(|s| s.session.agent_id == agent_id)
            .map(|s| s.session.clone())
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
        let sender = pending
            .remove(request_id)
            .ok_or_else(|| AgentCommsError::RequestNotFound(request_id.to_string()))?;
        sender.send(response.to_string()).map_err(|_| {
            AgentCommsError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Send failed",
            ))
        })?;
        Ok(true)
    }

    /// Cancel a pending input request, causing the agent to receive an error.
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
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| AgentCommsError::LockPoisoned)?;
        let session_id = sessions
            .values()
            .find(|s| s.session.agent_id == agent_id)
            .map(|s| s.session.id.clone());
        drop(sessions);

        if let Some(id) = session_id {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| AgentCommsError::LockPoisoned)?;
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
    ) -> std::sync::mpsc::Receiver<String> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(1);
        let mut pending = self.pending_input.lock().unwrap();
        pending.insert(request_id.to_string(), tx);
        rx
    }

    #[cfg(test)]
    pub(crate) fn pending_input_is_empty(&self) -> bool {
        self.pending_input.lock().unwrap().is_empty()
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
    pending_input: Arc<Mutex<HashMap<String, SyncSender<String>>>>,
    token: String,
    event_emitter: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    log::info!("Agent comms: new connection from {}", peer);

    let (tx, _rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1024);
    let reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(e) => {
            log::error!("failed to clone stream: {}", e);
            return;
        }
    };

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let msg: AgentMessage = match serde_json::from_str(&trimmed) {
            Ok(m) => m,
            Err(_) => continue,
        };

        handle_incoming_message(
            &stream,
            msg,
            &sessions,
            &pending_input,
            &token,
            &event_emitter,
            &tx,
        );
    }

    cleanup_connection(&stream, &sessions, &event_emitter);
    log::info!("Agent comms: connection closed from {}", peer);
}

fn handle_incoming_message(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, SyncSender<String>>>>,
    token: &str,
    event_emitter: &Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
    tx: &SyncSender<Vec<u8>>,
) {
    match msg.method.as_str() {
        "initialize" => handle_initialize(stream, msg, sessions, token, event_emitter, tx),
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
) {
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
        return;
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
    pending_input: &Arc<Mutex<HashMap<String, SyncSender<String>>>>,
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
            map.insert(request_id.clone(), input_tx);
        }

        match input_rx.recv_timeout(std::time::Duration::from_secs(5 * 60)) {
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
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
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
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Sender was dropped, most likely by cancel_input_request.
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
        let rx = comms.inject_input_request("req-001");

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
        let rx = comms.inject_input_request("req-002");

        let result = comms.respond_to_input_request("req-002", "hello");
        assert!(result.is_ok());
        assert!(comms.pending_input_is_empty());

        let response = rx.recv_timeout(std::time::Duration::from_millis(100));
        assert_eq!(response.unwrap(), "hello");
    }
}
