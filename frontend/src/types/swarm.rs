use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::workspace::AgentType;

/// Role an agent plays in the swarm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display, Default)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum AgentRole {
    #[default]
    Coordinator,
    Builder,
    Scout,
    Reviewer,
}

/// Status of a task within the swarm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum SwarmTaskStatus {
    Queued,
    Building,
    Review,
    Done,
    Blocked,
    Stalled,
}

/// Live status of a swarm agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum SwarmAgentStatus {
    Idle,
    Thinking,
    Writing,
    Waiting,
    Done,
    Blocked,
    Stalled,
}

/// Overall status of a swarm session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum SwarmStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
}

/// A task tracked by the swarm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub assigned_agent_id: String,
    pub owned_files: Vec<String>,
    pub status: SwarmTaskStatus,
    pub depends_on: Vec<String>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    pub last_updated_at: i64,
}

/// An agent participating in the swarm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwarmAgent {
    pub id: String,
    pub role: AgentRole,
    pub agent_type: AgentType,
    pub pane_id: String,
    pub status: SwarmAgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub last_action: String,
    pub last_action_at: i64,
}

/// A mailbox message between swarm agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: i64,
    pub read: bool,
}

/// Full state of a swarm session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwarmState {
    pub id: String,
    pub goal: String,
    pub agents: Vec<SwarmAgent>,
    pub tasks: Vec<SwarmTask>,
    pub messages: Vec<MailboxMessage>,
    pub status: SwarmStatus,
    pub started_at: i64,
}
