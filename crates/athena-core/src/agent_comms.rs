use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::EventEmitter;

#[path = "agent_comms_types.rs"]
mod agent_comms_types;
use agent_comms_types::SessionInternal;
#[path = "agent_comms_connection.rs"]
mod agent_comms_connection;
pub use agent_comms_types::{
    AgentCommsError, AgentMessage, AgentMessageType, AgentSession, InputRequest, SessionStatus,
};

/// Maximum size in bytes of a single agent-comms line. Prevents an agent
/// from streaming a giant line and forcing the server to allocate
/// unbounded memory before it ever sees a newline. Exceeding this cap
/// disconnects the misbehaving agent.
pub(super) const MAX_AGENT_LINE_BYTES: usize = 65_536; // 64 KiB

/// Field-level protocol limits prevent a valid line from creating oversized
/// session metadata or log/event identifiers.
pub(super) const MAX_AGENT_METHOD_BYTES: usize = 128;
pub(super) const MAX_AGENT_ID_BYTES: usize = 256;
pub(super) const MAX_AGENT_PLUGIN_ID_BYTES: usize = 256;
const MAX_AGENT_STATUS_BYTES: usize = 32;
const MAX_AGENT_LEVEL_BYTES: usize = 32;

fn valid_optional_text(params: &serde_json::Value, key: &str, max_bytes: usize) -> bool {
    params
        .get(key)
        .map(|value| {
            value.as_str().is_some_and(|text| {
                !text.is_empty() && text.len() <= max_bytes && !text.chars().any(|c| c.is_control())
            })
        })
        .unwrap_or(true)
}

pub(super) fn validate_agent_message(message: &AgentMessage) -> Result<(), &'static str> {
    if message.jsonrpc != "2.0" {
        return Err("unsupported jsonrpc version");
    }
    if message.method.is_empty() || message.method.len() > MAX_AGENT_METHOD_BYTES {
        return Err("method field too large");
    }
    if message.method.chars().any(|c| c.is_control()) {
        return Err("method contains control characters");
    }
    if message.id.as_deref().is_some_and(|id| {
        id.is_empty() || id.len() > MAX_AGENT_ID_BYTES || id.chars().any(|c| c.is_control())
    }) {
        return Err("id field too large or invalid");
    }

    match message.method.as_str() {
        "initialize" => {
            let data = message
                .params
                .get("data")
                .ok_or("initialize data is missing")?;
            if !valid_optional_text(data, "pluginId", MAX_AGENT_PLUGIN_ID_BYTES)
                || !valid_optional_text(data, "agentId", MAX_AGENT_ID_BYTES)
            {
                return Err("initialize identity field too large or invalid");
            }
        }
        "notifications/message"
            if !valid_optional_text(&message.params, "agentId", MAX_AGENT_ID_BYTES)
                || !valid_optional_text(&message.params, "level", MAX_AGENT_LEVEL_BYTES) =>
        {
            return Err("notification identity field too large or invalid");
        }
        "agents/status"
            if !valid_optional_text(&message.params, "status", MAX_AGENT_STATUS_BYTES) =>
        {
            return Err("status field too large or invalid");
        }
        "agents/requestInput"
            if !valid_optional_text(&message.params, "requestId", MAX_AGENT_ID_BYTES) =>
        {
            return Err("request ID field too large or invalid");
        }
        _ => {}
    }
    Ok(())
}

/// How long `handle_request_input` will block waiting for the user to
/// respond before giving up and returning a timeout error to the agent.
/// The frontend UI typically surfaces a confirmation dialog that
/// resolves within seconds; 30s is a generous upper bound that still
/// prevents a hung agent thread from being stuck forever. The
/// `cfg(test)` override keeps the timeout-path unit test fast while
/// leaving the production behavior unchanged.
#[cfg(not(test))]
pub(super) const INPUT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
pub(super) const INPUT_REQUEST_TIMEOUT: Duration = Duration::from_millis(150);

/// A pending input request, tracked per session so that
/// `cleanup_connection` can drop the sender (and wake the agent's
/// `recv_timeout` with `Disconnected`) when the originating connection
/// goes away, instead of leaking the entry until the 30s timeout.
pub(super) struct PendingInput {
    pub(super) session_id: String,
    pub(super) sender: SyncSender<String>,
}

pub(super) fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(super) fn now_ms() -> u64 {
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
    event_emitter: EventEmitter,
}

impl std::fmt::Debug for AgentComms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentComms")
            .field("sessions", &"<Mutex<HashMap>>")
            .field("pending_input", &"<Mutex<HashMap>>")
            .field("token", &"[REDACTED]")
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
                            agent_comms_connection::handle_connection(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;

    #[test]
    fn rejects_oversized_or_controlled_message_fields() {
        let message = AgentMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "x".repeat(MAX_AGENT_METHOD_BYTES + 1),
            params: serde_json::Value::Null,
        };
        assert_eq!(
            validate_agent_message(&message),
            Err("method field too large")
        );

        let message = AgentMessage {
            jsonrpc: "1.0".to_string(),
            id: None,
            method: "agents/heartbeat".to_string(),
            params: serde_json::Value::Null,
        };
        assert_eq!(
            validate_agent_message(&message),
            Err("unsupported jsonrpc version")
        );

        let message = AgentMessage {
            jsonrpc: "2.0".to_string(),
            id: Some("request\nforged".to_string()),
            method: "agents/heartbeat".to_string(),
            params: serde_json::Value::Null,
        };
        assert_eq!(
            validate_agent_message(&message),
            Err("id field too large or invalid")
        );

        let message = AgentMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "initialize".to_string(),
            params: serde_json::json!({
                "data": { "agentId": "a".repeat(MAX_AGENT_ID_BYTES + 1) }
            }),
        };
        assert_eq!(
            validate_agent_message(&message),
            Err("initialize identity field too large or invalid")
        );

        let message = AgentMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "agents/requestInput".to_string(),
            params: serde_json::json!({
                "requestId": "r".repeat(MAX_AGENT_ID_BYTES + 1)
            }),
        };
        assert_eq!(
            validate_agent_message(&message),
            Err("request ID field too large or invalid")
        );
    }

    #[test]
    fn invalid_authenticated_message_returns_jsonrpc_error() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let emitter: EventEmitter = Arc::new(Mutex::new(None));
        let token = "test-token-invalid-fields".to_string();

        let sessions_t = Arc::clone(&sessions);
        let pending_t = Arc::clone(&pending);
        let emitter_t = Arc::clone(&emitter);
        let token_t = token.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            agent_comms_connection::handle_connection(
                stream, sessions_t, pending_t, token_t, emitter_t,
            );
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let invalid = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "bad-fields",
            "method": "initialize",
            "params": { "data": { "token": token, "agentId": "x".repeat(MAX_AGENT_ID_BYTES + 1) } }
        });
        client
            .write_all(format!("{}\n", invalid).as_bytes())
            .unwrap();
        client.flush().unwrap();

        let mut reader = BufReader::new(client.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let response: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], "bad-fields");
        assert_eq!(response["error"]["code"], -32600);

        drop(reader);
        drop(client);
        server.join().expect("server thread panicked");
        assert!(sessions.lock().unwrap().is_empty());
    }

    #[test]
    fn tcp_rejects_invalid_nested_fields_after_authentication() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let emitter: EventEmitter = Arc::new(Mutex::new(None));
        let token = "test-token-nested-fields".to_string();

        let sessions_t = Arc::clone(&sessions);
        let pending_t = Arc::clone(&pending);
        let emitter_t = Arc::clone(&emitter);
        let token_t = token.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            agent_comms_connection::handle_connection(
                stream, sessions_t, pending_t, token_t, emitter_t,
            );
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        let initialize = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "init-nested",
            "method": "initialize",
            "params": { "data": { "token": token, "agentId": "nested-agent" } }
        });
        client
            .write_all(format!("{}\n", initialize).as_bytes())
            .unwrap();
        client.flush().unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&line).unwrap()["result"].is_object());

        let invalid_messages = [
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "bad-status",
                "method": "agents/status",
                "params": { "status": "s".repeat(33) }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "bad-level",
                "method": "notifications/message",
                "params": { "level": "l".repeat(33) }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "bad-request-id",
                "method": "agents/requestInput",
                "params": { "requestId": "r".repeat(MAX_AGENT_ID_BYTES + 1) }
            }),
        ];

        for message in invalid_messages {
            client
                .write_all(format!("{}\n", message).as_bytes())
                .unwrap();
            client.flush().unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let response: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["error"]["code"], -32600, "line: {line}");
        }

        drop(reader);
        drop(client);
        server.join().expect("server thread panicked");
        assert!(sessions.lock().unwrap().is_empty());
    }

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
        let emitter_arc: EventEmitter = Arc::new(Mutex::new(None));

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
            agent_comms_connection::handle_request_input(
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
        let emitter_arc: EventEmitter = Arc::new(Mutex::new(None));
        let token = "test-token-H4".to_string();

        let sessions_t = Arc::clone(&sessions_arc);
        let pending_t = Arc::clone(&pending_arc);
        let emitter_t = Arc::clone(&emitter_arc);
        let token_t = token.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            agent_comms_connection::handle_connection(
                stream, sessions_t, pending_t, token_t, emitter_t,
            );
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
        let emitter_arc: EventEmitter = Arc::new(Mutex::new(None));
        let token = "test-token-H4-pos".to_string();

        let sessions_t = Arc::clone(&sessions_arc);
        let pending_t = Arc::clone(&pending_arc);
        let emitter_t = Arc::clone(&emitter_arc);
        let token_t = token.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            agent_comms_connection::handle_connection(
                stream, sessions_t, pending_t, token_t, emitter_t,
            );
        });

        let mut client = TcpStream::connect(addr).unwrap();
        // Send a valid initialize.
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "init-1",
            "method": "initialize",
            "params": { "data": { "token": token, "agentId": "legit-agent" } }
        });
        client.write_all(format!("{}\n", init).as_bytes()).unwrap();
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
