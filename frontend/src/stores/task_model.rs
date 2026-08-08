//! Pure kanban task model: status mapping and JSON parsing.

use std::str::FromStr;

use crate::stores::workspace::AgentType;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Kanban column status.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum KanbanStatus {
    #[default]
    Todo,
    InProgress,
    InReview,
    Complete,
}

/// A single task on the kanban board.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KanbanTask {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub description: Option<String>,
    pub assigned_agent: Option<AgentType>,
    pub status: KanbanStatus,
    pub order: usize,
    pub created_at: i64,
}

// ---------------------------------------------------------------------------
// Backend ↔ Frontend mapping
// ---------------------------------------------------------------------------

/// Map a backend status string (PascalCase, e.g. "InProgress") to KanbanStatus.
/// Falls back to Todo on unrecognized values.
pub fn status_from_backend(s: &str) -> KanbanStatus {
    match s {
        "Todo" => KanbanStatus::Todo,
        "InProgress" => KanbanStatus::InProgress,
        "InReview" => KanbanStatus::InReview,
        "Complete" => KanbanStatus::Complete,
        _ => KanbanStatus::Todo,
    }
}

/// Map a KanbanStatus to the snake_case string the backend `parse()` accepts.
pub fn status_to_backend(status: &KanbanStatus) -> &'static str {
    match status {
        KanbanStatus::Todo => "todo",
        KanbanStatus::InProgress => "in_progress",
        KanbanStatus::InReview => "in_review",
        KanbanStatus::Complete => "complete",
    }
}

/// Parse a JSON array of backend tasks (as returned by `kanban_get_tasks`)
/// into frontend `KanbanTask`s.
pub fn tasks_from_backend_json(json: &str) -> Result<Vec<KanbanTask>, String> {
    let values: Vec<serde_json::Value> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(values
        .into_iter()
        .map(|v| {
            let id = v
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let space_id = v
                .get("space_id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let title = v
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let description = v
                .get("description")
                .and_then(|x| x.as_str())
                .map(String::from);
            let assigned_agent = v
                .get("assigned_agent")
                .and_then(|x| x.as_str())
                .and_then(|s| AgentType::from_str(&s.to_lowercase()).ok());
            let status = v
                .get("status")
                .and_then(|x| x.as_str())
                .map(status_from_backend)
                .unwrap_or_default();
            let order = v.get("order").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            let created_at = v.get("created_at").and_then(|x| x.as_i64()).unwrap_or(0);
            KanbanTask {
                id,
                space_id,
                title,
                description,
                assigned_agent,
                status,
                order,
                created_at,
            }
        })
        .collect())
}
