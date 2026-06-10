use super::kanban_column::KanbanColumn;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::task::{use_task_store, KanbanStatus, KanbanTask};
use dioxus::prelude::*;

#[component]
pub fn KanbanBoard() -> Element {
    let task_state = use_task_store();

    let columns = [
        ("To Do", KanbanStatus::Todo),
        ("In Progress", KanbanStatus::InProgress),
        ("Review", KanbanStatus::InReview),
        ("Done", KanbanStatus::Complete),
    ];

    let is_empty = task_state.read().tasks.is_empty();

    rsx! {
        div {
            class: "kanban-board",
            style: "display: flex; height: 100%; gap: 12px; padding: 12px; overflow-x: auto; background: transparent; color: var(--text);",

            if is_empty {
                EmptyState {
                    kind: EmptyArt::Kanban,
                    title: "No tasks".to_string(),
                    hint: Some("Add a task to a column to start planning.".to_string()),
                }
            } else {
                for (col_name, col_status) in columns.iter() {
                    {
                        // Map store's KanbanTask to the component's KanbanTask prop type
                        let col_tasks: Vec<KanbanTask> = task_state.read()
                            .tasks
                            .iter()
                            .filter(|t| t.status == *col_status)
                            .cloned()
                            .collect();
                        let col_name_owned = col_name.to_string();
                        rsx! {
                            KanbanColumn {
                                key: "{col_name}",
                                title: col_name_owned,
                                tasks: col_tasks,
                            }
                        }
                    }
                }
            }
        }
    }
}
