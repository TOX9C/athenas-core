use super::kanban_card::KanbanCard;
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
            style: "display: flex; flex-direction: column; min-width: 240px; max-width: 280px; background: var(--bgSecondary); border-radius: 8px; overflow: hidden;",

            // Column header
            div {
                style: "padding: 10px 12px; border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between;",
                span {
                    style: "font-size: 12px; font-weight: 600; color: var(--text);",
                    "{props.title}"
                }
                span {
                    style: "font-size: 10px; padding: 1px 5px; border-radius: 3px; background: var(--bgTertiary); color: var(--textDim);",
                    "{props.tasks.len()}"
                }
            }

            // Cards
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 6px;",

                for task in props.tasks.iter() {
                    KanbanCard { key: "{task.id}", task: task.clone() }
                }
            }

            // Add task input
            div {
                style: "padding: 8px; border-top: 1px solid var(--border);",
                div {
                    style: "display: flex; gap: 4px;",
                    input {
                        style: "flex: 1; padding: 4px 8px; border-radius: 4px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 10px; outline: none;",
                        value: "{add_text}",
                        oninput: move |e| add_text.set(e.value()),
                        placeholder: "Add task..."
                    }
                    button {
                        style: "padding: 4px 8px; border-radius: 4px; border: none; background: var(--accent); color: #fff; cursor: pointer; font-size: 10px;",
                        onclick: move |_| {
                            // TODO: add task via store
                            add_text.set(String::new());
                        },
                        "+"
                    }
                }
            }
        }
    }
}
