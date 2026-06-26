use super::activity_feed::SwarmActivityFeed;
use super::agent_card::AgentCard;
use crate::components::shared::icon::IconSwarm;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::swarm::{
    use_swarm_store, MailboxMessage, SwarmAgentStatus, SwarmOverallStatus, SwarmTaskStatus,
};
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

#[component]
pub fn SwarmBoard() -> Element {
    let swarm_state = use_swarm_store();
    let mut mounted = use_signal(|| false);
    let unlisten: Rc<RefCell<Option<Box<dyn FnOnce()>>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let unlisten_clone = unlisten.clone();

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        let mut store = swarm_state;

        // swarm:stateChange — Refresh swarm state display.
        if let Ok(u) = tauri_bridge::listen("swarm:stateChange", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                // Check if it's a full state replacement.
                if val.get("id").is_some() {
                    let id = val
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let goal = val
                        .get("goal")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let status_str = val
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("active");
                    let status = match status_str {
                        "paused" => SwarmOverallStatus::Paused,
                        "completed" => SwarmOverallStatus::Completed,
                        _ => SwarmOverallStatus::Active,
                    };
                    let started_at = val.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);

                    // Parse agents.
                    let agents: Vec<crate::stores::swarm::SwarmAgent> = val
                        .get("agents")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|a| {
                                    let id = a.get("id").and_then(|v| v.as_str())?.to_string();
                                    let pane_id = a
                                        .get("paneId")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let last_action = a
                                        .get("lastAction")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let last_action_at =
                                        a.get("lastActionAt").and_then(|v| v.as_i64()).unwrap_or(0);
                                    let status_str =
                                        a.get("status").and_then(|v| v.as_str()).unwrap_or("idle");
                                    let agent_status = match status_str {
                                        "thinking" => SwarmAgentStatus::Thinking,
                                        "writing" => SwarmAgentStatus::Writing,
                                        "waiting" => SwarmAgentStatus::Waiting,
                                        "done" => SwarmAgentStatus::Done,
                                        "blocked" => SwarmAgentStatus::Blocked,
                                        "stalled" => SwarmAgentStatus::Stalled,
                                        _ => SwarmAgentStatus::Idle,
                                    };
                                    Some(crate::stores::swarm::SwarmAgent {
                                        id,
                                        role: crate::stores::swarm::AgentRole::default(),
                                        agent_type: crate::stores::workspace::AgentType::Shell,
                                        pane_id,
                                        status: agent_status,
                                        current_task: None,
                                        last_action,
                                        last_action_at,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    // Parse tasks.
                    let tasks: Vec<crate::stores::swarm::SwarmTask> = val
                        .get("tasks")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|t| {
                                    let id = t.get("id").and_then(|v| v.as_str())?.to_string();
                                    let title =
                                        t.get("title").and_then(|v| v.as_str())?.to_string();
                                    let description = t
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let assigned_agent_id = t
                                        .get("assignedAgentId")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let created_at =
                                        t.get("createdAt").and_then(|v| v.as_i64()).unwrap_or(0);
                                    let last_updated_at = t
                                        .get("lastUpdatedAt")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or(0);
                                    let status_str = t
                                        .get("status")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("queued");
                                    let task_status = match status_str {
                                        "building" => SwarmTaskStatus::Building,
                                        "review" => SwarmTaskStatus::Review,
                                        "done" => SwarmTaskStatus::Done,
                                        "blocked" => SwarmTaskStatus::Blocked,
                                        "stalled" => SwarmTaskStatus::Stalled,
                                        _ => SwarmTaskStatus::Queued,
                                    };
                                    Some(crate::stores::swarm::SwarmTask {
                                        id,
                                        title,
                                        description,
                                        assigned_agent_id,
                                        owned_files: Vec::new(),
                                        status: task_status,
                                        depends_on: Vec::new(),
                                        created_at,
                                        completed_at: None,
                                        last_updated_at,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    // Parse messages.
                    let messages: Vec<MailboxMessage> = val
                        .get("messages")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|m| {
                                    let id = m.get("id").and_then(|v| v.as_str())?.to_string();
                                    let from = m.get("from").and_then(|v| v.as_str())?.to_string();
                                    let to = m.get("to").and_then(|v| v.as_str())?.to_string();
                                    let content =
                                        m.get("content").and_then(|v| v.as_str())?.to_string();
                                    let timestamp =
                                        m.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                                    let read =
                                        m.get("read").and_then(|v| v.as_bool()).unwrap_or(false);
                                    Some(MailboxMessage {
                                        id,
                                        from,
                                        to,
                                        content,
                                        timestamp,
                                        read,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    let swarm_data = crate::stores::swarm::SwarmData {
                        id,
                        goal,
                        agents,
                        tasks,
                        messages,
                        status,
                        started_at,
                    };
                    store.write().replace_swarm(swarm_data);
                } else if let Some(agent_id) = val.get("agentId").and_then(|v| v.as_str()) {
                    // Partial update: agent status change.
                    let status_str = val.get("status").and_then(|v| v.as_str()).unwrap_or("idle");
                    let agent_status = match status_str {
                        "thinking" => SwarmAgentStatus::Thinking,
                        "writing" => SwarmAgentStatus::Writing,
                        "waiting" => SwarmAgentStatus::Waiting,
                        "done" => SwarmAgentStatus::Done,
                        "blocked" => SwarmAgentStatus::Blocked,
                        "stalled" => SwarmAgentStatus::Stalled,
                        _ => SwarmAgentStatus::Idle,
                    };
                    store.write().update_agent_status(agent_id, agent_status);
                } else if val.get("mailboxMessage").is_some() {
                    // Mailbox message.
                    let mb = val.get("mailboxMessage");
                    if let Some(msg_obj) = mb.and_then(|v| v.as_object()) {
                        let id = msg_obj
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let from = msg_obj
                            .get("from")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let to = msg_obj
                            .get("to")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let content = msg_obj
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let timestamp = msg_obj
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        let read = msg_obj
                            .get("read")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        store.write().add_mailbox_message(MailboxMessage {
                            id,
                            from,
                            to,
                            content,
                            timestamp,
                            read,
                        });
                    }
                }
            }
        }) {
            *unlisten_clone.borrow_mut() = Some(u);
        }
    });

    // Cleanup: unlisten on component unmount.
    let unlisten_drop = unlisten.clone();
    use_drop(move || {
        if let Some(u) = unlisten_drop.borrow_mut().take() {
            u();
        }
    });

    let (agents, activities) = match &swarm_state.read().active_swarm {
        Some(swarm) => {
            let agents = swarm.agents.clone();
            let activities: Vec<ActivityEntry> = swarm
                .messages
                .iter()
                .map(|m| ActivityEntry {
                    id: m.id.clone(),
                    agent_name: m.from.clone(),
                    role: "agent".to_string(),
                    action: m.content.clone(),
                    timestamp: m.timestamp,
                })
                .collect();
            (agents, activities)
        }
        None => (Vec::new(), Vec::new()),
    };

    rsx! {
        div {
            class: "swarm-board",
            style: "display: flex; height: 100%; background: var(--bg); color: var(--text);",

            // Agent cards grid
            div {
                style: "flex: 1; padding: 16px; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column;",

                div {
                    style: "display: flex; align-items: center; gap: 8px; margin-bottom: 14px;",
                    IconSwarm { size: Some(18), color: Some("var(--accent)".to_string()) }
                    span {
                        style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; letter-spacing: 0.01em; color: var(--text);",
                        "Swarm"
                    }
                }

                if agents.is_empty() {
                    EmptyState {
                        kind: EmptyArt::Swarm,
                        title: "No swarm".to_string(),
                        hint: Some("Launch a swarm to coordinate agents.".to_string()),
                    }
                } else {
                    div {
                        style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 12px;",
                        for agent in agents.iter() {
                            AgentCard { key: "{agent.id}", agent: agent.clone() }
                        }
                    }
                }
            }

            // Activity feed sidebar
            div {
                style: "width: 280px; border-left: 1px solid var(--border); background: var(--bgSecondary);",
                SwarmActivityFeed { activities: activities }
            }
        }
    }
}
