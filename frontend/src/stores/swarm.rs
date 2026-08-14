use dioxus::prelude::*;

#[path = "swarm_model.rs"]
mod swarm_model;

pub use crate::types::swarm::AgentRole;
pub use swarm_model::{
    parse_swarm_data, MailboxMessage, SwarmAgent, SwarmAgentStatus, SwarmData, SwarmOverallStatus,
    SwarmTask, SwarmTaskStatus,
};

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
