use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during kanban operations.
#[derive(Debug, Error)]
pub enum KanbanError {
    #[error("Task not found: {0}")]
    NotFound(String),
    #[error("Store error: {0}")]
    Store(#[from] athena_store::StoreError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Workspace not found")]
    WorkspaceNotFound,
    #[error("Invalid status: {0}")]
    InvalidStatus(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Kanban task status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum KanbanBackendStatus {
    #[default]
    Todo,
    InProgress,
    InReview,
    Complete,
}

/// A single Kanban task persisted to the backend KeyValueStore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanBackendTask {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub description: Option<String>,
    pub assigned_agent: Option<String>,
    pub status: KanbanBackendStatus,
    pub order: usize,
    pub created_at: i64,
    /// Optional back-link to the plan step this card was created from
    /// (Kanban ↔ plan deep link). `None` for manually-created cards.
    /// `#[serde(default)]` keeps older persisted JSON (without the field)
    /// deserializable.
    #[serde(default)]
    pub plan_step_id: Option<String>,
}

impl KanbanBackendStatus {
    /// Parse a status string into a `KanbanBackendStatus`.
    pub fn parse(s: &str) -> Result<Self, KanbanError> {
        match s.to_lowercase().as_str() {
            "todo" | "to do" => Ok(Self::Todo),
            "in_progress" | "in progress" | "inprogress" => Ok(Self::InProgress),
            "in_review" | "in review" | "inreview" => Ok(Self::InReview),
            "complete" | "done" | "completed" => Ok(Self::Complete),
            _ => Err(KanbanError::InvalidStatus(s.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// KanbanBackend
// ---------------------------------------------------------------------------

/// Backend for kanban operations using a `KeyValueStore`.
pub struct KanbanBackend {
    store: Arc<athena_store::KeyValueStore>,
}

impl KanbanBackend {
    pub fn new(store: Arc<athena_store::KeyValueStore>) -> Self {
        Self { store }
    }

    fn key(&self, workspace_id: &str) -> String {
        format!("kanban.{workspace_id}")
    }

    /// Get all tasks for a workspace.
    pub fn get_tasks(&self, workspace_id: &str) -> Result<Vec<KanbanBackendTask>, KanbanError> {
        let key = self.key(workspace_id);
        let tasks_json = self.store.get::<String>(&key)?;
        match tasks_json {
            None => Ok(Vec::new()),
            Some(json) => Ok(serde_json::from_str(&json)?),
        }
    }

    /// Save all tasks for a workspace.
    pub fn save_tasks(
        &self,
        workspace_id: &str,
        tasks: &[KanbanBackendTask],
    ) -> Result<(), KanbanError> {
        let key = self.key(workspace_id);
        let json = serde_json::to_string(tasks)?;
        self.store.set_sync(&key, &json)?;
        Ok(())
    }

    /// Create a new task in the given workspace.
    pub fn create_task(
        &self,
        workspace_id: &str,
        task: KanbanBackendTask,
    ) -> Result<KanbanBackendTask, KanbanError> {
        let mut tasks = self.get_tasks(workspace_id)?;
        tasks.push(task.clone());
        self.save_tasks(workspace_id, &tasks)?;
        Ok(task)
    }

    /// Update an existing task by ID.
    pub fn update_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        title: Option<String>,
        description: Option<String>,
        status: Option<KanbanBackendStatus>,
    ) -> Result<KanbanBackendTask, KanbanError> {
        let mut tasks = self.get_tasks(workspace_id)?;
        let task = tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| KanbanError::NotFound(task_id.to_string()))?;
        if let Some(t) = title {
            task.title = t;
        }
        if let Some(d) = description {
            task.description = if d.is_empty() { None } else { Some(d) };
        }
        if let Some(s) = status {
            task.status = s;
        }
        let updated = task.clone();
        self.save_tasks(workspace_id, &tasks)?;
        Ok(updated)
    }

    /// Delete a task by ID.
    pub fn delete_task(&self, workspace_id: &str, task_id: &str) -> Result<(), KanbanError> {
        let mut tasks = self.get_tasks(workspace_id)?;
        let original_len = tasks.len();
        tasks.retain(|t| t.id != task_id);
        if tasks.len() == original_len {
            return Err(KanbanError::NotFound(task_id.to_string()));
        }
        self.save_tasks(workspace_id, &tasks)?;
        Ok(())
    }

    /// Read the active workspace ID from the store.
    pub fn get_active_workspace_id(&self) -> Result<String, KanbanError> {
        let json = self
            .store
            .get::<String>("workspaces")?
            .ok_or(KanbanError::WorkspaceNotFound)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let id = value
            .get("active_space_id")
            .and_then(|v| v.as_str())
            .ok_or(KanbanError::WorkspaceNotFound)?
            .to_string();
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend() -> (KanbanBackend, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let _path = tmp.path().join("test.json");
        let store = athena_store::KeyValueStore::with_name_sync("test")
            .unwrap_or_else(|_| athena_store::KeyValueStore::new_empty());
        (KanbanBackend::new(Arc::new(store)), tmp)
    }

    #[test]
    fn crud_round_trip() {
        let (backend, _tmp) = make_backend();
        let task = KanbanBackendTask {
            id: "t1".to_string(),
            space_id: "s1".to_string(),
            title: "Test task".to_string(),
            description: Some("desc".to_string()),
            assigned_agent: None,
            status: KanbanBackendStatus::Todo,
            order: 0,
            created_at: 0,
            plan_step_id: None,
        };
        backend.create_task("ws1", task.clone()).unwrap();
        let tasks = backend.get_tasks("ws1").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Test task");

        backend
            .update_task("ws1", "t1", Some("Updated".to_string()), None, None)
            .unwrap();
        let tasks = backend.get_tasks("ws1").unwrap();
        assert_eq!(tasks[0].title, "Updated");

        backend.delete_task("ws1", "t1").unwrap();
        let tasks = backend.get_tasks("ws1").unwrap();
        assert!(tasks.is_empty());
    }
}
