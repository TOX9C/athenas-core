use crate::components::shared::icon::{IconEdit, IconTrash};
use crate::stores::athena::DraggableItem;
use crate::stores::task::KanbanTask;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct KanbanCardProps {
    pub task: KanbanTask,
}

#[component]
pub fn KanbanCard(props: KanbanCardProps) -> Element {
    let mut is_editing = use_signal(|| false);

    let accent_color = match &props.task.assigned_agent {
        Some(_) => "var(--accent)",
        None => "var(--textDim)",
    };

    rsx! {
        div {
            class: "kanban-card card is-interactive",
            style: "border-left: 3px solid {accent_color}; cursor: grab;",
            draggable: "true",
            ondragstart: move |e| {
                let dt = e.data_transfer();
                let status_str = match &props.task.status {
                    crate::stores::task::KanbanStatus::Todo => "todo",
                    crate::stores::task::KanbanStatus::InProgress => "in_progress",
                    crate::stores::task::KanbanStatus::InReview => "in_review",
                    crate::stores::task::KanbanStatus::Complete => "complete",
                };
                let item = DraggableItem::KanbanTask {
                    task_id: props.task.id.clone(),
                    title: props.task.title.clone(),
                    status: status_str.to_string(),
                };
                if let Ok(json) = serde_json::to_string(&item) {
                    let _ = dt.set_data("text/plain", &json);
                }
            },

            div {
                style: "display: flex; align-items: center; gap: 6px;",

                span {
                    style: "font-size: var(--text-xs); font-weight: 500; color: var(--text); flex: 1;",
                    "{props.task.title}"
                }

                // Actions
                button {
                    class: "icon-btn",
                    title: "Edit",
                    onmouseenter: move |_| {},
                    onmouseleave: move |_| {},
                    onclick: move |_| is_editing.set(!is_editing()),
                    IconEdit { size: Some(14), color: Some("currentColor".to_string()) }
                }
                button {
                    class: "icon-btn",
                    title: "Delete",
                    onmouseenter: move |_| {},
                    onmouseleave: move |_| {},
                    onclick: move |_| {
                        // TODO: delete task via store
                    },
                    IconTrash { size: Some(14), color: Some("var(--error)".to_string()) }
                }
            }

            if let Some(ref desc) = props.task.description {
                if !desc.is_empty() {
                    div {
                        style: "font-size: var(--text-xs); color: var(--textMuted); margin-top: 6px; line-height: 1.4;",
                        "{desc}"
                    }
                }
            }
        }
    }
}
