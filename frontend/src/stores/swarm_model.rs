//! Pure swarm coordination contracts.

use crate::stores::workspace::AgentType;
use crate::types::swarm::AgentRole;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Status of a swarm task.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwarmOverallStatus {
    #[default]
    Active,
    Paused,
    Completed,
    Cancelled,
}

/// A task within a swarm.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: i64,
    pub read: bool,
}

/// The full state of an active swarm.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwarmData {
    pub id: String,
    pub workspace_dir: String,
    pub goal: String,
    pub agents: Vec<SwarmAgent>,
    pub tasks: Vec<SwarmTask>,
    pub messages: Vec<MailboxMessage>,
    pub status: SwarmOverallStatus,
    pub started_at: i64,
    #[serde(default)]
    pub revision: u64,
}

/// Parse the backend's canonical camelCase JSON contract into the frontend
/// model. Unknown enum values are downgraded to safe defaults so a newer
/// agent process cannot make the board disappear during a rolling upgrade.
pub fn parse_swarm_data(raw: &str) -> Result<SwarmData, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let workspace_dir = value
        .get("workspaceDir")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let goal = value
        .get("goal")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let status = match value
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("active")
    {
        "paused" => SwarmOverallStatus::Paused,
        "completed" => SwarmOverallStatus::Completed,
        "cancelled" => SwarmOverallStatus::Cancelled,
        _ => SwarmOverallStatus::Active,
    };
    let agents = value
        .get("agents")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(|v| v.as_str())?.to_string();
                    Some(SwarmAgent {
                        id,
                        role: item
                            .get("role")
                            .and_then(|v| v.as_str())
                            .and_then(parse_role)
                            .unwrap_or_default(),
                        agent_type: item
                            .get("agentType")
                            .and_then(|v| v.as_str())
                            .and_then(parse_agent_type)
                            .unwrap_or_default(),
                        pane_id: item
                            .get("paneId")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        status: parse_agent_status(item.get("status").and_then(|v| v.as_str())),
                        current_task: item
                            .get("currentTask")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned),
                        last_action: item
                            .get("lastAction")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        last_action_at: item
                            .get("lastActionAt")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let tasks = value
        .get("tasks")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(SwarmTask {
                        id: item.get("id").and_then(|v| v.as_str())?.to_string(),
                        title: item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        description: item
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        assigned_agent_id: item
                            .get("assignedAgentId")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        owned_files: item
                            .get("ownedFiles")
                            .and_then(|v| v.as_array())
                            .map(|files| {
                                files
                                    .iter()
                                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        status: parse_task_status(item.get("status").and_then(|v| v.as_str())),
                        depends_on: item
                            .get("dependsOn")
                            .and_then(|v| v.as_array())
                            .map(|deps| {
                                deps.iter()
                                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        created_at: item
                            .get("createdAt")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_default(),
                        completed_at: item.get("completedAt").and_then(|v| v.as_i64()),
                        last_updated_at: item
                            .get("lastUpdatedAt")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let messages = value
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(MailboxMessage {
                        id: item.get("id").and_then(|v| v.as_str())?.to_string(),
                        from: item
                            .get("from")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        to: item
                            .get("to")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        content: item
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        timestamp: item
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_default(),
                        read: item.get("read").and_then(|v| v.as_bool()).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SwarmData {
        id,
        workspace_dir,
        goal,
        agents,
        tasks,
        messages,
        status,
        started_at: value
            .get("startedAt")
            .and_then(|v| v.as_i64())
            .unwrap_or_default(),
        revision: value
            .get("revision")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
    })
}

fn parse_role(value: &str) -> Option<AgentRole> {
    AgentRole::from_str(&value.to_ascii_lowercase()).ok()
}

fn parse_agent_type(value: &str) -> Option<AgentType> {
    AgentType::from_str(&value.to_ascii_lowercase()).ok()
}

fn parse_agent_status(value: Option<&str>) -> SwarmAgentStatus {
    match value.unwrap_or("idle") {
        "thinking" => SwarmAgentStatus::Thinking,
        "writing" => SwarmAgentStatus::Writing,
        "waiting" => SwarmAgentStatus::Waiting,
        "done" => SwarmAgentStatus::Done,
        "blocked" => SwarmAgentStatus::Blocked,
        "stalled" => SwarmAgentStatus::Stalled,
        _ => SwarmAgentStatus::Idle,
    }
}

fn parse_task_status(value: Option<&str>) -> SwarmTaskStatus {
    match value.unwrap_or("queued") {
        "building" => SwarmTaskStatus::Building,
        "review" => SwarmTaskStatus::Review,
        "done" => SwarmTaskStatus::Done,
        "blocked" => SwarmTaskStatus::Blocked,
        "stalled" => SwarmTaskStatus::Stalled,
        _ => SwarmTaskStatus::Queued,
    }
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
    fn parser_preserves_canonical_nested_fields() {
        let raw = r#"{
            "id":"swarm-1","goal":"Improve","status":"active","startedAt":42,"revision":3,
            "agents":[{"id":"agent-1","role":"reviewer","agentType":"codex","paneId":"pane-1","status":"writing","currentTask":"task-1","lastAction":"Reviewing","lastActionAt":41}],
            "tasks":[{"id":"task-1","title":"Inspect","description":"Read code","assignedAgentId":"agent-1","ownedFiles":["src/lib.rs"],"status":"review","dependsOn":["task-0"],"createdAt":40,"completedAt":null,"lastUpdatedAt":41}],
            "messages":[{"id":"msg-1","from":"agent-1","to":"agent-2","content":"done","timestamp":41,"read":false}]
        }"#;
        let data = parse_swarm_data(raw).unwrap();
        assert_eq!(data.revision, 3);
        assert_eq!(data.agents[0].role, AgentRole::Reviewer);
        assert_eq!(data.agents[0].agent_type, AgentType::Codex);
        assert_eq!(data.agents[0].current_task.as_deref(), Some("task-1"));
        assert_eq!(data.tasks[0].owned_files, vec!["src/lib.rs"]);
        assert_eq!(data.tasks[0].depends_on, vec!["task-0"]);
        assert_eq!(data.messages[0].content, "done");
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
            workspace_dir: String::new(),
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
