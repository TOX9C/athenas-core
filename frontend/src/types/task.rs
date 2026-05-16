use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::workspace::AgentType;

/// Status of a kanban task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum KanbanStatus {
    Todo,
    InProgress,
    InReview,
    Complete,
}

/// A kanban board task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KanbanTask {
    pub id: String,
    pub space_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<AgentType>,
    pub status: KanbanStatus,
    pub order: usize,
    pub created_at: i64,
}
