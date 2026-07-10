use crate::components::shared::icon::{IconEdit, IconTrash};
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
            // Flat card — opaque fill, hairline border, gold-edge hover via
            // .lit-sweep. The left accent rule keeps the assigned/unassigned
            // distinction (accent_color stays the same source of truth).
            // Class + handlers byte-identical — only the visual layer retuned.
            class: "kanban-card card is-interactive lit-sweep",
            style: "border-left: 3px solid {accent_color}; border-top: 1px solid var(--border); border-right: 1px solid var(--border); border-bottom: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); cursor: grab;",

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
