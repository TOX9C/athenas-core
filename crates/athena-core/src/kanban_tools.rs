//! Kanban tool implementations for [`ToolExecutor`].

use super::{ToolCallResult, ToolExecutor, ToolExecutorError, ToolInput};
use crate::kanban::{KanbanBackendStatus, KanbanBackendTask};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

impl ToolExecutor {
    pub(super) fn get_current_time_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    pub(super) fn kanban_list_tasks(&self) -> Result<ToolCallResult, ToolExecutorError> {
        let workspace_id = match self.kanban_backend.get_active_workspace_id() {
            Ok(id) => id,
            Err(_) => {
                return Ok(ToolCallResult {
                    text: "No active workspace found.".to_string(),
                    is_error: None,
                })
            }
        };

        let tasks = match self.kanban_backend.get_tasks(&workspace_id) {
            Ok(tasks) => tasks,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error reading kanban tasks: {}", e),
                    is_error: Some(true),
                })
            }
        };

        if tasks.is_empty() {
            return Ok(ToolCallResult {
                text: "No tasks found on the Kanban board.".to_string(),
                is_error: None,
            });
        }

        let json = match serde_json::to_string(&tasks) {
            Ok(j) => j,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error serializing tasks: {}", e),
                    is_error: Some(true),
                })
            }
        };

        Ok(ToolCallResult {
            text: json,
            is_error: None,
        })
    }

    pub(super) fn kanban_create_task(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let title = args
            .title
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("title".to_string()))?;
        let space_id = args
            .space_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("space_id".to_string()))?;

        let status = match args.status.as_deref() {
            Some(s) => KanbanBackendStatus::parse(s).unwrap_or(KanbanBackendStatus::Todo),
            None => KanbanBackendStatus::Todo,
        };

        let task = KanbanBackendTask {
            id: format!("task-{}", Uuid::new_v4()),
            space_id: space_id.to_string(),
            title: title.to_string(),
            description: args.description.clone(),
            assigned_agent: None,
            status,
            order: 0,
            created_at: self.get_current_time_ms(),
        };

        match self.kanban_backend.create_task(space_id, task) {
            Ok(created) => Ok(ToolCallResult {
                text: format!("Task created: {} (ID: {})", created.title, created.id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error creating task: {}", e),
                is_error: Some(true),
            }),
        }
    }

    pub(super) fn kanban_update_task(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let task_id = args
            .task_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("task_id".to_string()))?;

        let workspace_id = match self.kanban_backend.get_active_workspace_id() {
            Ok(id) => id,
            Err(_) => {
                return Ok(ToolCallResult {
                    text: "No active workspace found.".to_string(),
                    is_error: Some(true),
                })
            }
        };

        let status = args
            .status
            .as_ref()
            .and_then(|s| KanbanBackendStatus::parse(s).ok());

        match self.kanban_backend.update_task(
            &workspace_id,
            task_id,
            args.title.clone(),
            args.description.clone(),
            status,
        ) {
            Ok(updated) => Ok(ToolCallResult {
                text: format!("Task updated: {} (ID: {})", updated.title, updated.id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error updating task: {}", e),
                is_error: Some(true),
            }),
        }
    }

    pub(super) fn kanban_delete_task(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let task_id = args
            .task_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("task_id".to_string()))?;

        let workspace_id = match self.kanban_backend.get_active_workspace_id() {
            Ok(id) => id,
            Err(_) => {
                return Ok(ToolCallResult {
                    text: "No active workspace found.".to_string(),
                    is_error: Some(true),
                })
            }
        };

        match self.kanban_backend.delete_task(&workspace_id, task_id) {
            Ok(_) => Ok(ToolCallResult {
                text: format!("Task {} deleted.", task_id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error deleting task: {}", e),
                is_error: Some(true),
            }),
        }
    }
}
