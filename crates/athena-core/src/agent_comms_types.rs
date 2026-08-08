//! Agent communications protocol and session data contracts.

use std::net::SocketAddr;
use std::sync::mpsc::SyncSender;
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
pub(super) struct SessionInternal {
    pub(super) session: AgentSession,
    pub(super) sender: SyncSender<Vec<u8>>,
    pub(super) peer_addr: Option<SocketAddr>,
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
