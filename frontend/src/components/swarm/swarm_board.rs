use super::activity_feed::SwarmActivityFeed;
use super::agent_card::AgentCard;
use crate::components::shared::icon::IconSwarm;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::swarm::{parse_swarm_data, use_swarm_store, SwarmOverallStatus};
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Activity feed entry derived from swarm mailbox messages.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActivityEntry {
    pub id: String,
    pub agent_name: String,
    pub role: String,
    pub action: String,
    pub timestamp: i64,
}

fn status_label(status: &SwarmOverallStatus) -> &'static str {
    match status {
        SwarmOverallStatus::Active => "active",
        SwarmOverallStatus::Paused => "paused",
        SwarmOverallStatus::Completed => "completed",
        SwarmOverallStatus::Cancelled => "cancelled",
    }
}

#[component]
pub fn SwarmBoard() -> Element {
    let swarm_state = use_swarm_store();
    let workspace = use_workspace_store();
    let mut task_title = use_signal(String::new);
    let mut task_description = use_signal(String::new);
    let mut message_text = use_signal(String::new);
    let unlisten: Rc<RefCell<Option<Box<dyn FnOnce()>>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let watched_dir: Rc<RefCell<Option<String>>> = use_hook(|| Rc::new(RefCell::new(None)));

    // Load persisted state whenever the active workspace changes. This also
    // recovers a mission after an app restart, before the first watcher tick.
    use_effect({
        let watched_dir = watched_dir.clone();
        let mut store = swarm_state;
        move || {
            let state = workspace.read();
            let dir = state
                .active_space_id
                .as_ref()
                .and_then(|id| state.spaces.iter().find(|space| &space.id == id))
                .map(|space| space.dir.clone());
            let Some(dir) = dir else {
                store.write().set_swarm(None);
                if let Some(previous_dir) = watched_dir.borrow_mut().take() {
                    spawn(async move {
                        let _ = tauri_bridge::swarm_stop_watch(&previous_dir).await;
                    });
                }
                return;
            };
            let previous_dir = watched_dir.borrow_mut().replace(dir.clone());
            if let Some(previous_dir) = previous_dir.filter(|previous| previous != &dir) {
                spawn(async move {
                    let _ = tauri_bridge::swarm_stop_watch(&previous_dir).await;
                });
            }
            spawn(async move {
                match tauri_bridge::swarm_read_state(&dir).await {
                    Ok(raw) if raw.trim() != "null" => {
                        match parse_swarm_data(&raw) {
                            Ok(data) if data.workspace_dir == dir => {
                                store.write().replace_swarm(data)
                            }
                            Ok(data) => web_sys::console::warn_1(
                                &format!(
                                    "[SwarmBoard] ignoring state for unexpected workspace: {:?}",
                                    data.workspace_dir
                                )
                                .into(),
                            ),
                            Err(error) => web_sys::console::warn_1(
                                &format!("[SwarmBoard] state parse failed: {error}").into(),
                            ),
                        }
                        if let Err(error) = tauri_bridge::swarm_start_watch(&dir).await {
                            web_sys::console::warn_1(
                                &format!("[SwarmBoard] watcher start failed: {:?}", error).into(),
                            );
                        }
                    }
                    Ok(_) => store.write().set_swarm(None),
                    Err(error) => web_sys::console::warn_1(
                        &format!("[SwarmBoard] state load failed: {:?}", error).into(),
                    ),
                }
            });
        }
    });

    // Subscribe once to the canonical full-state event. Every mutation emits
    // the same complete contract, so the board never has to merge partial
    // payloads or guess which fields changed.
    let unlisten_effect = unlisten.clone();
    use_effect(move || {
        if unlisten_effect.borrow().is_some() {
            return;
        }
        let mut store = swarm_state;
        let current_workspace = workspace;
        if let Ok(unlisten_fn) =
            tauri_bridge::listen("swarm:stateChange", move |payload: String| {
                if let Ok(data) = parse_swarm_data(&payload) {
                    let active_dir = {
                        let state = current_workspace.read();
                        state
                            .active_space_id
                            .as_ref()
                            .and_then(|id| {
                                state
                                    .spaces
                                    .iter()
                                    .find(|space| &space.id == id)
                                    .map(|space| space.dir.as_str())
                            })
                            .map(str::to_string)
                    };
                    if !data.workspace_dir.is_empty()
                        && active_dir.as_deref() == Some(data.workspace_dir.as_str())
                    {
                        store.write().replace_swarm(data);
                    }
                }
            })
        {
            *unlisten_effect.borrow_mut() = Some(unlisten_fn);
        }
    });

    let unlisten_drop = unlisten.clone();
    let watched_dir_drop = watched_dir.clone();
    use_drop(move || {
        if let Some(unlisten_fn) = unlisten_drop.borrow_mut().take() {
            unlisten_fn();
        }
        if let Some(dir) = watched_dir_drop.borrow_mut().take() {
            spawn(async move {
                let _ = tauri_bridge::swarm_stop_watch(&dir).await;
            });
        }
    });

    let active_dir = {
        let state = workspace.read();
        state
            .active_space_id
            .as_ref()
            .and_then(|id| state.spaces.iter().find(|space| &space.id == id))
            .map(|space| space.dir.clone())
    };
    let active_swarm = swarm_state.read().active_swarm.clone();
    let (agents, tasks, activities, status, goal) = match active_swarm {
        Some(swarm) => {
            let activities = swarm
                .messages
                .iter()
                .map(|message| ActivityEntry {
                    id: message.id.clone(),
                    agent_name: message.from.clone(),
                    role: "agent".to_string(),
                    action: message.content.clone(),
                    timestamp: message.timestamp,
                })
                .collect();
            (
                swarm.agents,
                swarm.tasks,
                activities,
                status_label(&swarm.status),
                swarm.goal,
            )
        }
        None => (Vec::new(), Vec::new(), Vec::new(), "idle", String::new()),
    };

    let first_agent = agents.first().map(|agent| agent.id.clone());
    let second_agent = agents.get(1).map(|agent| agent.id.clone());
    let can_add_task =
        active_dir.is_some() && first_agent.is_some() && !task_title().trim().is_empty();
    let can_send_message = active_dir.is_some()
        && first_agent.is_some()
        && second_agent.is_some()
        && !message_text().trim().is_empty();
    let dir_for_controls = active_dir.clone();
    let pause_dir = active_dir.clone().unwrap_or_default();
    let complete_dir = active_dir.clone().unwrap_or_default();
    let dir_for_task = active_dir.clone();
    let dir_for_message = active_dir.clone();
    let first_for_task = first_agent.clone();
    let first_for_message = first_agent.clone();
    let second_for_message = second_agent.clone();

    rsx! {
        div {
            class: "swarm-board",
            style: "display: flex; height: 100%; background: var(--bg); color: var(--text);",
            div {
                class: "swarm-main",
                style: "flex: 1; padding: 16px; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; gap: 12px;",                    div { style: "display: flex; align-items: center; gap: 8px; margin-bottom: 2px;",

                    IconSwarm { size: Some(18), color: Some("var(--accent)".to_string()) }
                    span {
                        style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; color: var(--text);",
                        "Swarm"
                    }
                    span {
                        style: "font-size: var(--text-xs); color: var(--accent); text-transform: uppercase; letter-spacing: .08em;",
                        "{status}"
                    }
                    div { style: "flex: 1;" }
                    if dir_for_controls.is_some() {
                        button {
                            class: "btn-ghost",
                            disabled: status == "completed" || status == "cancelled",
                            onclick: move |_| {
                                let next = if status == "paused" { "active" } else { "paused" };
                                let dir = pause_dir.clone();
                                spawn(async move { let _ = tauri_bridge::swarm_set_status(&dir, next).await; });
                            },
                            if status == "paused" { "Resume" } else { "Pause" }
                        }
                        button {
                            class: "btn-ghost",
                            disabled: status == "completed" || status == "cancelled",
                            onclick: move |_| {
                                let dir = complete_dir.clone();
                                spawn(async move { let _ = tauri_bridge::swarm_set_status(&dir, "completed").await; });
                            },
                            "Complete"
                        }
                    }
                }
                if !goal.is_empty() {
                    div { style: "padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--textMuted); font-size: var(--text-sm);", "{goal}" }
                }
                if agents.is_empty() {
                    EmptyState {
                        kind: EmptyArt::Swarm,
                        title: "No swarm".to_string(),
                        hint: Some("Launch a swarm to coordinate agents.".to_string()),
                    }
                } else {
                    div {
                        class: "swarm-network",
                        style: "display: flex; flex-direction: column; gap: 12px; padding: 14px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary);",
                        div {
                            style: "display: flex; align-items: baseline; justify-content: space-between; gap: 12px;",
                            div {
                                style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; color: var(--text);",
                                "Agent roster"
                            }
                            span {
                                style: "font-size: var(--text-xs); color: var(--textMuted);",
                                "{agents.len()} agents working toward this mission"
                            }
                        }
                        div {
                            class: "swarm-agent-grid",
                            style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(168px, 1fr)); gap: 10px;",
                            for agent in agents.iter() {
                                AgentCard { key: "{agent.id}", agent: agent.clone() }
                            }
                        }
                    }
                }
                div {
                    style: "display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 10px;",
                    div { style: "padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgSecondary);",
                        div { style: "font-size: var(--text-xs); color: var(--accent); text-transform: uppercase; margin-bottom: 8px;", "Tasks" }
                        for task in tasks.iter() {
                            div { key: "{task.id}", style: "display: flex; align-items: center; gap: 8px; padding: 5px 0; border-top: 1px solid var(--border); font-size: var(--text-xs);",
                                span { style: "flex: 1;", "{task.title}" }
                                span { style: "color: var(--textMuted);", "{task.status:?}" }
                            }
                        }
                        if let Some(dir) = dir_for_task.clone() {
                            input { class: "field", value: "{task_title}", placeholder: "New task", oninput: move |event| task_title.set(event.value()) }
                            textarea { class: "field", style: "margin-top: 6px; min-height: 42px;", value: "{task_description}", placeholder: "Description (optional)", oninput: move |event| task_description.set(event.value()) }
                            button { class: "btn-primary", style: "margin-top: 6px;", disabled: !can_add_task,
                                onclick: move |_| {
                                    let dir = dir.clone();
                                    let agent = first_for_task.clone().unwrap_or_default();
                                    let title = task_title();
                                    let description = task_description();
                                    spawn(async move { let _ = tauri_bridge::swarm_create_task(&dir, &title, &description, &agent).await; });
                                    task_title.set(String::new());
                                    task_description.set(String::new());
                                },
                                "Add task"
                            }
                        }
                    }
                    div { style: "padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgSecondary);",
                        div { style: "font-size: var(--text-xs); color: var(--accent); text-transform: uppercase; margin-bottom: 8px;", "Send message" }
                        input { class: "field", value: "{message_text}", placeholder: "Message the next agent", oninput: move |event| message_text.set(event.value()) }
                        if let Some(dir) = dir_for_message.clone() {
                            button { class: "btn-primary", style: "margin-top: 6px;", disabled: !can_send_message,
                                onclick: move |_| {
                                    let dir = dir.clone();
                                    let from = first_for_message.clone().unwrap_or_default();
                                    let to = second_for_message.clone().unwrap_or_default();
                                    let content = message_text();
                                    spawn(async move { let _ = tauri_bridge::swarm_send_message(&dir, &from, &to, &content).await; });
                                    message_text.set(String::new());
                                },
                                "Send"
                            }
                        }
                    }
                }
            }
            div { style: "width: 280px; border-left: 1px solid var(--border); background: var(--bgSecondary);", SwarmActivityFeed { activities } }
        }
    }
}
