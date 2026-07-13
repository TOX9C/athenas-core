use super::kanban_column::KanbanColumn;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::task::{tasks_from_backend_json, use_task_store, KanbanStatus, KanbanTask};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[component]
pub fn KanbanBoard() -> Element {
    let task_state = use_task_store();
    let mut loaded = use_signal(|| false);

    let columns = [
        ("To Do", KanbanStatus::Todo),
        ("In Progress", KanbanStatus::InProgress),
        ("Review", KanbanStatus::InReview),
        ("Done", KanbanStatus::Complete),
    ];

    // Load tasks from backend on mount.
    use_effect(move || {
        if loaded() {
            return;
        }
        loaded.set(true);
        let mut task_state = task_state;
        spawn(async move {
            match tauri_bridge::kanban_get_tasks().await {
                Ok(json) => {
                    match tasks_from_backend_json(&json) {
                        Ok(tasks) => {
                            task_state.write().set_tasks(tasks);
                        }
                        Err(e) => {
                            web_sys::console::error_1(
                                &format!("[KanbanBoard] failed to parse tasks: {e}").into(),
                            );
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("[KanbanBoard] kanban_get_tasks failed: {:?}", e).into(),
                    );
                }
            }
        });
    });


    let is_empty = task_state.read().tasks.is_empty();

    rsx! {
        div {
            class: "kanban-board pane-astrolabe-mark",
            style: "display: flex; height: 100%; gap: 12px; padding: 12px; overflow-x: auto; background: var(--bg); color: var(--text);",

            if is_empty {
                EmptyState {
                    kind: EmptyArt::Kanban,
                    title: "No tasks".to_string(),
                    hint: Some("Add a task to a column to start planning.".to_string()),
                }
            } else {
                for (col_name, col_status) in columns.iter() {
                    {
                        let col_tasks: Vec<KanbanTask> = task_state.read()
                            .tasks
                            .iter()
                            .filter(|t| t.status == *col_status)
                            .cloned()
                            .collect();
                        let col_name_owned = col_name.to_string();
                        let closure_status = *col_status;
                        rsx! {
                            KanbanColumn {
                                key: "{col_name}",
                                title: col_name_owned,
                                tasks: col_tasks,
                                status: closure_status,
                            }
                        }
                    }
                }
            }
        }
    }
}
