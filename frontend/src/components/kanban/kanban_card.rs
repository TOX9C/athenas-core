use crate::components::shared::icon::{IconCheck, IconClose, IconEdit, IconTrash};
use crate::stores::task::{use_task_store, KanbanTask};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct KanbanCardProps {
    pub task: KanbanTask,
}

#[component]
pub fn KanbanCard(props: KanbanCardProps) -> Element {
    let mut is_editing = use_signal(|| false);
    let mut edit_text = use_signal(|| props.task.title.clone());
    let task_store = use_task_store();
    let id_for_keydown = props.task.id.clone();
    let id_for_save = props.task.id.clone();
    let id_for_delete = props.task.id.clone();

    let accent_color = match props.task.assigned_agent {
        Some(_) => "var(--accent)",
        None => "var(--textDim)",
    };

    rsx! {
        div {
            class: "kanban-card card is-interactive lit-sweep",
            style: "border-left: 3px solid {accent_color}; border-top: 1px solid var(--border); border-right: 1px solid var(--border); border-bottom: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); cursor: grab;",

            if *is_editing.read() {
                // Edit mode
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    input {
                        class: "field",
                        style: "flex: 1; font-size: var(--text-xs); font-weight: 500; padding: 4px 6px;",
                        value: "{edit_text}",
                        oninput: move |e| edit_text.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                let new_title = edit_text.read().clone();
                                let tid = id_for_keydown.clone();
                                let mut ts = task_store;
                                is_editing.set(false);
                                spawn(async move {
                                    let _ = tauri_bridge::kanban_update_task(
                                        &tid, Some(&new_title), None, None,
                                    ).await;
                                    if let Ok(json) = tauri_bridge::kanban_get_tasks().await {
                                        if let Ok(tasks) = crate::stores::task::tasks_from_backend_json(&json) {
                                            ts.write().set_tasks(tasks);
                                        }
                                    }
                                });
                            } else if e.key() == Key::Escape {
                                is_editing.set(false);
                            }
                        },
                    }
                    button {
                        class: "icon-btn",
                        title: "Save",
                        onclick: move |_| {
                            let new_title = edit_text.read().clone();
                            let tid = id_for_save.clone();
                            let mut ts = task_store;
                            is_editing.set(false);
                            spawn(async move {
                                let _ = tauri_bridge::kanban_update_task(
                                    &tid, Some(&new_title), None, None,
                                ).await;
                                if let Ok(json) = tauri_bridge::kanban_get_tasks().await {
                                    if let Ok(tasks) = crate::stores::task::tasks_from_backend_json(&json) {
                                        ts.write().set_tasks(tasks);
                                    }
                                }
                            });
                        },
                        IconCheck { size: Some(14), color: Some("var(--success)".to_string()) }
                    }
                    button {
                        class: "icon-btn",
                        title: "Cancel",
                        onclick: move |_| is_editing.set(false),
                        IconClose { size: Some(14), color: Some("var(--textDim)".to_string()) }
                    }
                }
            } else {
                // Display mode
                div {
                    style: "display: flex; align-items: center; gap: 6px;",

                    span {
                        style: "font-size: var(--text-xs); font-weight: 500; color: var(--text); flex: 1;",
                        "{props.task.title}"
                    }

                    button {
                        class: "icon-btn",
                        title: "Edit",
                        onclick: move |_| {
                            edit_text.set(props.task.title.clone());
                            is_editing.set(true);
                        },
                        IconEdit { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                    button {
                        class: "icon-btn",
                        title: "Delete",
                        onclick: move |_| {
                            let tid = id_for_delete.clone();
                            let mut ts = task_store;
                            spawn(async move {
                                let _ = tauri_bridge::kanban_delete_task(&tid).await;
                                if let Ok(json) = tauri_bridge::kanban_get_tasks().await {
                                    if let Ok(tasks) = crate::stores::task::tasks_from_backend_json(&json) {
                                        ts.write().set_tasks(tasks);
                                    }
                                }
                            });
                        },
                        IconTrash { size: Some(14), color: Some("var(--error)".to_string()) }
                    }
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
