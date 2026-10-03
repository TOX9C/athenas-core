use crate::components::shared::confirm_dialog::ConfirmDialog;
use crate::components::shared::icon::{
    IconCheck, IconChevronDown, IconClose, IconEdit, IconKanban, IconTrash,
};
use crate::stores::athena::use_athena_store;
use crate::stores::task::{status_to_backend, use_task_store, KanbanStatus, KanbanTask};
use crate::stores::ui::{use_ui_store, Panel};
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
    let mut show_move_menu = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let task_store = use_task_store();
    let ui_state = use_ui_store();
    let athena_state = use_athena_store();
    let id_for_keydown = props.task.id.clone();
    let id_for_save = props.task.id.clone();
    let id_for_delete = props.task.id.clone();
    let title_for_confirm = props.task.title.clone();
    let plan_step_id_for_link = props.task.plan_step_id.clone();
    let current_status = props.task.status;
    let task_id_for_move = props.task.id.clone();

    rsx! {
        div {
            class: "kanban-card card lit-sweep",
            style: "position: relative; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgSecondary);",

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
                        title: "Move to column",
                        "aria-label": "Move to column",
                        "aria-expanded": "{show_move_menu()}",
                        onclick: move |e| {
                            e.stop_propagation();
                            show_move_menu.set(!show_move_menu());
                        },
                        IconChevronDown { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                    button {
                        class: "icon-btn",
                        title: "Delete",
                        style: "color: var(--error);",
                        onclick: move |_| confirm_delete.set(true),
                        IconTrash { size: Some(14), color: Some("currentColor".to_string()) }
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

            // Kanban ↔ plan deep link: cards created from a plan step carry
            // the step id; clicking jumps to the Athena plan and pulses it.
            if plan_step_id_for_link.is_some() {
                button {
                    class: "badge",
                    style: "display: inline-flex; align-items: center; gap: 4px; margin-top: 8px; cursor: pointer; color: var(--accent); background: var(--accentSubtle);",
                    title: "Jump to this step in the plan",
                    onclick: move |_| {
                        let step_id = plan_step_id_for_link.clone();
                        let mut ui_state = ui_state;
                        let mut athena_state = athena_state;
                        spawn(async move {
                            ui_state.write().panel = Panel::Chat;
                            athena_state.write().set_open(true);
                            if let Some(sid) = step_id {
                                athena_state.write().set_plan_highlight(Some(sid));
                            }
                        });
                    },
                    IconKanban { size: Some(11), color: Some("currentColor".to_string()) }
                    "View in plan"
                }
            }

            // Destructive-action confirmation: deletes are immediate in the
            // backend with no undo, so gate them behind an explicit confirm.
            if confirm_delete() {
                ConfirmDialog {
                    title: "Delete task".to_string(),
                    message: format!("Delete \"{title_for_confirm}\"? This cannot be undone."),
                    confirm_label: "Delete".to_string(),
                    on_cancel: move |_| confirm_delete.set(false),
                    on_confirm: move |_| {
                        confirm_delete.set(false);
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
                }
            }

            // Move-to-column menu — the only way to change a card's status
            // from the UI (agents can also move cards via their tools).
            if show_move_menu() {
                div {
                    style: "position: fixed; inset: 0; z-index: 20;",
                    onclick: move |_| show_move_menu.set(false),
                }
                div {
                    style: "position: absolute; top: 34px; right: 6px; z-index: 30; min-width: 150px; padding: 4px; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md); box-shadow: var(--shadow-lg); display: flex; flex-direction: column; gap: 1px; transform-origin: top; animation: pop-in 140ms var(--ease) both;",
                    for (name, status) in [
                        ("To Do", KanbanStatus::Todo),
                        ("In Progress", KanbanStatus::InProgress),
                        ("Review", KanbanStatus::InReview),
                        ("Done", KanbanStatus::Complete),
                    ] {
                        {
                            let is_current = status == current_status;
                            let item_color = if is_current { "var(--accent)" } else { "var(--text)" };
                            let item_weight = if is_current { "600" } else { "500" };
                            let target_status = status;
                            let task_id_for_move = task_id_for_move.clone();
                            rsx! {
                                button {
                                    key: "move-{name}",
                                    style: "display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 10px; border: none; background: transparent; border-radius: var(--radius-sm); color: {item_color}; font-size: var(--text-xs); font-weight: {item_weight}; text-align: left; cursor: pointer; transition: background-color var(--dur-fast) var(--ease);",
                                    onclick: move |_| {
                                        show_move_menu.set(false);
                                        if is_current { return; }
                                        move_task_to_column(task_id_for_move.clone(), target_status, task_store);
                                    },
                                    span { style: "width: 10px; flex-shrink: 0;", if is_current { "\u{2713}" } else { "" } }
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Move a kanban card to another column via the backend, then reload tasks so
/// the store remains the single source of truth.
fn move_task_to_column(
    task_id: String,
    target: KanbanStatus,
    mut task_store: Signal<crate::stores::task::TaskState>,
) {
    let status_str = status_to_backend(&target).to_string();
    spawn(async move {
        let _ = tauri_bridge::kanban_update_task(&task_id, None, None, Some(&status_str)).await;
        if let Ok(json) = tauri_bridge::kanban_get_tasks().await {
            if let Ok(tasks) = crate::stores::task::tasks_from_backend_json(&json) {
                task_store.write().set_tasks(tasks);
            }
        }
    });
}
