use dioxus::prelude::*;

use super::workspace::AgentType;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Kanban column status.
#[derive(Debug, Clone, PartialEq, Default)]
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

/// Maximum tasks kept in memory.
const MAX_TASKS: usize = 200;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global task / kanban state.
#[derive(Clone, PartialEq, Default)]
pub struct TaskState {
    pub tasks: Vec<KanbanTask>,
}

impl TaskState {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn add_task(&mut self, task: KanbanTask) {
        self.tasks.push(task);
        if self.tasks.len() > MAX_TASKS {
            let excess = self.tasks.len() - MAX_TASKS;
            self.tasks.drain(0..excess);
        }
    }

    pub fn remove_task(&mut self, id: &str) {
        self.tasks.retain(|t| t.id != id);
    }

    pub fn update_task(&mut self, id: &str, f: impl FnOnce(&mut KanbanTask)) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            f(task);
        }
    }

    pub fn move_task(&mut self, id: &str, status: KanbanStatus) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.status = status;
        }
    }

    pub fn set_tasks(&mut self, tasks: Vec<KanbanTask>) {
        if tasks.len() > MAX_TASKS {
            self.tasks = tasks[tasks.len() - MAX_TASKS..].to_vec();
        } else {
            self.tasks = tasks;
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the task signal from the Dioxus context.
pub fn use_task_store() -> Signal<TaskState> {
    use_context::<Signal<TaskState>>()
}

/// Initialize the task store as a context provider.
pub fn provide_task_store() {
    use_context_provider(|| Signal::new(TaskState::new()));
}
