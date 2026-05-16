use crate::stores::task::KanbanTask;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct KanbanCardProps {
    pub task: KanbanTask,
}

#[component]
pub fn KanbanCard(props: KanbanCardProps) -> Element {
    let mut is_editing = use_signal(|| false);

    let status_color = match &props.task.assigned_agent {
        Some(_) => "var(--accent)",
        None => "var(--textDim)",
    };

    rsx! {
        div {
            class: "kanban-card",
            style: "padding: 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); cursor: grab;",

            div {
                style: "display: flex; align-items: center; gap: 4px;",

                div {
                    style: "width: 6px; height: 6px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                }

                span {
                    style: "font-size: 11px; font-weight: 500; color: var(--text); flex: 1;",
                    "{props.task.title}"
                }

                // Actions
                button {
                    style: "padding: 2px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 10px;",
                    onclick: move |_| is_editing.set(!is_editing()),
                    "\u{270f}" // edit
                }
                button {
                    style: "padding: 2px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 10px;",
                    onclick: move |_| {
                        // TODO: delete task via store
                    },
                    "\u{00d7}"
                }
            }

            if let Some(ref desc) = props.task.description {
                if !desc.is_empty() {
                    div {
                        style: "font-size: 10px; color: var(--textDim); margin-top: 4px;",
                        "{desc}"
                    }
                }
            }

            if is_editing() {
                div {
                    style: "margin-top: 6px; font-size: 9px; color: var(--accent);",
                    "TODO: edit modal"
                }
            }
        }
    }
}
