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
    /// Plan-step back-link (Kanban ↔ plan deep link): set when the card was
    /// created from a plan step, so the card can jump back to the plan.
    pub plan_step_id: Option<String>,
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
            let plan_step_id = v
                .get("plan_step_id")
                .and_then(|x| x.as_str())
                .map(String::from);
            KanbanTask {
                id,
                space_id,
                title,
                description,
                assigned_agent,
                status,
                order,
                created_at,
                plan_step_id,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract: backend status strings round-trip through the frontend enum
    // without loss; unrecognized values degrade to Todo, never panic.
    #[test]
    fn status_mapping_round_trips_all_columns() {
        for (backend, expected) in [
            ("Todo", KanbanStatus::Todo),
            ("InProgress", KanbanStatus::InProgress),
            ("InReview", KanbanStatus::InReview),
            ("Complete", KanbanStatus::Complete),
        ] {
            assert_eq!(status_from_backend(backend), expected);
            assert_eq!(
                status_to_backend(&expected),
                match expected {
                    KanbanStatus::Todo => "todo",
                    KanbanStatus::InProgress => "in_progress",
                    KanbanStatus::InReview => "in_review",
                    KanbanStatus::Complete => "complete",
                }
            );
        }
    }

    #[test]
    fn unknown_backend_status_falls_back_to_todo() {
        assert_eq!(status_from_backend("Archived"), KanbanStatus::Todo);
        assert_eq!(status_from_backend(""), KanbanStatus::Todo);
        assert_eq!(status_from_backend("inprogress"), KanbanStatus::Todo);
    }

    #[test]
    fn tasks_parse_from_backend_json_with_all_fields() {
        let json = r#"[
            {
                "id": "task-1",
                "space_id": "space-9",
                "title": "Ship it",
                "description": "final pass",
                "assigned_agent": "claude",
                "status": "InProgress",
                "order": 3,
                "created_at": 1724000000,
                "plan_step_id": "step-7"
            }
        ]"#;
        let tasks = tasks_from_backend_json(json).unwrap();
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(task.id, "task-1");
        assert_eq!(task.space_id, "space-9");
        assert_eq!(task.title, "Ship it");
        assert_eq!(task.description.as_deref(), Some("final pass"));
        assert_eq!(task.assigned_agent, Some(AgentType::Claude));
        assert_eq!(task.status, KanbanStatus::InProgress);
        assert_eq!(task.order, 3);
        assert_eq!(task.created_at, 1724000000);
        assert_eq!(task.plan_step_id.as_deref(), Some("step-7"));
    }

    #[test]
    fn tasks_parse_tolerates_missing_optional_fields() {
        let json = r#"[{ "id": "task-2", "title": "Minimal" }]"#;
        let tasks = tasks_from_backend_json(json).unwrap();
        let task = &tasks[0];
        assert_eq!(task.space_id, "");
        assert_eq!(task.description, None);
        assert_eq!(task.assigned_agent, None);
        assert_eq!(task.status, KanbanStatus::Todo);
        assert_eq!(task.order, 0);
        assert_eq!(task.created_at, 0);
        assert_eq!(task.plan_step_id, None);
    }

    #[test]
    fn unknown_agent_string_parses_to_none_not_error() {
        let json = r#"[{ "id": "t", "title": "t", "assigned_agent": "skynet" }]"#;
        let tasks = tasks_from_backend_json(json).unwrap();
        assert_eq!(tasks[0].assigned_agent, None);
    }

    #[test]
    fn malformed_json_is_an_error_not_panic() {
        assert!(tasks_from_backend_json("not json").is_err());
        assert!(tasks_from_backend_json("{").is_err());
    }
}
