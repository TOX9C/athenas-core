use dioxus::prelude::*;

use super::workspace::AgentType;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Role a swarm agent plays.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AgentRole {
    #[default]
    Coordinator,
    Builder,
    Scout,
    Reviewer,
}

/// Status of a swarm task.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SwarmTaskStatus {
    #[default]
    Queued,
    Building,
    Review,
    Done,
    Blocked,
    Stalled,
}

/// Status of a swarm agent.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SwarmAgentStatus {
    #[default]
    Idle,
    Thinking,
    Writing,
    Waiting,
    Done,
    Blocked,
    Stalled,
}

/// Overall status of a swarm.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SwarmOverallStatus {
    #[default]
    Active,
    Paused,
    Completed,
}

/// A task within a swarm.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SwarmTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub assigned_agent_id: String,
    pub owned_files: Vec<String>,
    pub status: SwarmTaskStatus,
    pub depends_on: Vec<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub last_updated_at: i64,
}

/// An agent participating in a swarm.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SwarmAgent {
    pub id: String,
    pub role: AgentRole,
    pub agent_type: AgentType,
    pub pane_id: String,
    pub status: SwarmAgentStatus,
    pub current_task: Option<String>,
    pub last_action: String,
    pub last_action_at: i64,
}

/// A message in the swarm mailbox.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: i64,
    pub read: bool,
}

/// The full state of an active swarm.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SwarmData {
    pub id: String,
    pub goal: String,
    pub agents: Vec<SwarmAgent>,
    pub tasks: Vec<SwarmTask>,
    pub messages: Vec<MailboxMessage>,
    pub status: SwarmOverallStatus,
    pub started_at: i64,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global swarm coordination state.
#[derive(Clone, PartialEq, Default)]
pub struct SwarmState {
    pub active_swarm: Option<SwarmData>,
}

impl SwarmState {
    pub fn new() -> Self {
        Self { active_swarm: None }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn set_swarm(&mut self, swarm: Option<SwarmData>) {
        self.active_swarm = swarm;
    }

    /// Partially update the active swarm. If no swarm is active, this is a
    /// no-op.
    pub fn update_swarm(&mut self, f: impl FnOnce(&mut SwarmData)) {
        if let Some(swarm) = &mut self.active_swarm {
            f(swarm);
        }
    }

    /// Replace the active swarm state from a backend event payload.
    pub fn replace_swarm(&mut self, swarm: SwarmData) {
        self.active_swarm = Some(swarm);
    }

    /// Update an agent's status within the active swarm.
    pub fn update_agent_status(&mut self, agent_id: &str, status: SwarmAgentStatus) {
        if let Some(swarm) = &mut self.active_swarm {
            if let Some(agent) = swarm.agents.iter_mut().find(|a| a.id == agent_id) {
                agent.status = status;
            }
        }
    }

    /// Add a mailbox message to the active swarm.
    pub fn add_mailbox_message(&mut self, msg: MailboxMessage) {
        if let Some(swarm) = &mut self.active_swarm {
            swarm.messages.push(msg);
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the swarm signal from the Dioxus context.
pub fn use_swarm_store() -> Signal<SwarmState> {
    use_context::<Signal<SwarmState>>()
}

/// Initialize the swarm store as a context provider.
pub fn provide_swarm_store() {
    use_context_provider(|| Signal::new(SwarmState::new()));
}
