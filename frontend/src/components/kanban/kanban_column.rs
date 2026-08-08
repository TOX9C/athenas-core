use super::kanban_card::KanbanCard;
use crate::components::shared::icon::IconPlus;
use crate::stores::task::{status_to_backend, use_task_store, KanbanStatus, KanbanTask};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct KanbanColumnProps {
    pub title: String,
    pub tasks: Vec<KanbanTask>,
    pub status: KanbanStatus,
}

#[component]
pub fn KanbanColumn(props: KanbanColumnProps) -> Element {
    let mut add_text = use_signal(String::new);
    let task_store = use_task_store();
    let col_status = props.status;

    rsx! {
        div {
            class: "kanban-column",
            style: "display: flex; flex-direction: column; min-width: 248px; max-width: 288px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); overflow: hidden;",

            // Column header
            div {
                style: "padding: 10px 12px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 8px;",
                span {
                    style: "width: 3px; height: 14px; border-radius: var(--radius-pill); background: var(--accent); flex-shrink: 0;",
                }
                span {
                    style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; letter-spacing: 0.04em; color: var(--accent); flex: 1;",
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
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                let text = add_text.read().clone();
                                if !text.is_empty() {
                                    add_text.set(String::new());
                                    spawn(async move {
                                        if let Err(e) = create_task_in_column(&text, col_status).await {
                                            web_sys::console::error_1(&format!("[KanbanColumn] create failed: {e:?}").into());
                                        }
                                        reload_tasks(task_store).await;
                                    });
                                }
                            }
                        },
                        placeholder: "Add task..."
                    }
                    button {
                        class: "icon-btn",
                        title: "Add task",
                        onclick: move |_| {
                            let text = add_text.read().clone();
                            if text.is_empty() { return; }
                            add_text.set(String::new());
                            spawn(async move {
                                if let Err(e) = create_task_in_column(&text, col_status).await {
                                    web_sys::console::error_1(&format!("[KanbanColumn] create failed: {e:?}").into());
                                }
                                reload_tasks(task_store).await;
                            });
                        },
                        IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                }
            }
        }
    }
}

/// Create a task and set its status to match the column.
async fn create_task_in_column(title: &str, col_status: KanbanStatus) -> Result<(), String> {
    let json = tauri_bridge::kanban_create_task(title, None)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let parsed = crate::stores::task::tasks_from_backend_json(&format!("[{json}]"))?;
    let task_id = parsed.first().map(|t| t.id.clone());

    let Some(task_id) = task_id else {
        return Ok(());
    };

    if col_status != KanbanStatus::Todo {
        let status_str = status_to_backend(&col_status).to_string();
        let _ = tauri_bridge::kanban_update_task(&task_id, None, None, Some(&status_str)).await;
    }

    Ok(())
}
/// Reload all tasks from backend into the store.
async fn reload_tasks(mut task_store: Signal<crate::stores::task::TaskState>) {
    if let Ok(json) = tauri_bridge::kanban_get_tasks().await {
        if let Ok(tasks) = crate::stores::task::tasks_from_backend_json(&json) {
            task_store.write().set_tasks(tasks);
        }
    }
}
