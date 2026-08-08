//! Pure swarm coordination contracts.

use crate::stores::workspace::AgentType;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_default_to_initial_values() {
        assert_eq!(AgentRole::default(), AgentRole::Coordinator);
        assert_eq!(SwarmTaskStatus::default(), SwarmTaskStatus::Queued);
        assert_eq!(SwarmAgentStatus::default(), SwarmAgentStatus::Idle);
        assert_eq!(SwarmOverallStatus::default(), SwarmOverallStatus::Active);
    }

    #[test]
    fn swarm_data_preserves_nested_contracts() {
        let task = SwarmTask {
            id: "task-1".to_string(),
            title: "Inspect code".to_string(),
            ..Default::default()
        };
        let agent = SwarmAgent {
            id: "agent-1".to_string(),
            role: AgentRole::Scout,
            agent_type: AgentType::Claude,
            ..Default::default()
        };
        let data = SwarmData {
            id: "swarm-1".to_string(),
            goal: "Improve structure".to_string(),
            agents: vec![agent],
            tasks: vec![task],
            ..Default::default()
        };
        assert_eq!(data.agents[0].id, "agent-1");
        assert_eq!(data.agents[0].agent_type, AgentType::Claude);
        assert_eq!(data.tasks[0].title, "Inspect code");
    }
}
