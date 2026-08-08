use dioxus::prelude::*;

#[path = "task_model.rs"]
mod task_model;

pub use task_model::{
    status_from_backend, status_to_backend, tasks_from_backend_json, KanbanStatus, KanbanTask,
};

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
