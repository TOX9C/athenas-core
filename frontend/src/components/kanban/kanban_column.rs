use super::kanban_card::KanbanCard;
use crate::components::shared::icon::IconPlus;
use crate::stores::task::KanbanTask;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct KanbanColumnProps {
    pub title: String,
    pub tasks: Vec<KanbanTask>,
}

#[component]
pub fn KanbanColumn(props: KanbanColumnProps) -> Element {
    let mut add_text = use_signal(String::new);

    rsx! {
        div {
            class: "kanban-column",
            style: "display: flex; flex-direction: column; min-width: 248px; max-width: 288px;",

            // Column header
            div {
                style: "padding: 10px 12px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 8px;",
                span {
                    style: "width: 3px; height: 14px; border-radius: var(--radius-pill); background: var(--accentSubtle); flex-shrink: 0;",
                }
                span {
                    style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; letter-spacing: 0.01em; color: var(--text); flex: 1;",
                    "{props.title}"
                }
                span {
                    class: "badge",
                    "{props.tasks.len()}"
                }
            }

            // Cards
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 8px;",

                for task in props.tasks.iter() {
                    KanbanCard { key: "{task.id}", task: task.clone() }
                }
            }

            // Add task input
            div {
                style: "padding: 8px; border-top: 1px solid var(--border);",
                div {
                    style: "display: flex; gap: 6px; align-items: center;",
                    input {
                        class: "field",
                        style: "flex: 1; padding: 6px 10px; font-size: var(--text-xs);",
                        value: "{add_text}",
                        oninput: move |e| add_text.set(e.value()),
                        placeholder: "Add task..."
                    }
                    button {
                        class: "icon-btn",
                        title: "Add task",
                        onclick: move |_| {
                            // TODO: add task via store
                            add_text.set(String::new());
                        },
                        IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                }
            }
        }
    }
}
