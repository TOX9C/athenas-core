use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus, AgentStatusUpdate};
use crate::stores::notification::{
    add_notification, use_notification_store, NotificationRecord, NotificationType,
};
use crate::stores::terminal::use_terminal_store;
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Output event bus component - renders nothing, handles IPC events.
///
/// Wires Tauri push events to the agent status and agent output stores:
/// 1. `agent:status:{pane_id}` - explicit status updates from the backend
/// 2. `terminal:exit:{pane_id}` - PTY exit transitions to Disconnected
/// 3. `agents:connected` - Add agent to status list
/// 4. `agents:disconnected` - Remove/update agent status
/// 5. `agents:statusUpdate` - Update agent status
/// 6. `agents:inputRequested` - Show input request
/// 7. `output-capture:line` - Append output line
/// 8. `output-capture:paneRegistered` - Register new pane
/// 9. `output-capture:paneUnregistered` - Remove pane
///
/// Also sets up heuristic shell-prompt detection: when terminal data arrives
/// containing a shell prompt pattern, the agent status transitions to Idle.
#[component]
pub fn OutputEventBus() -> Element {
    let agent_status = use_agent_status_store();
    let agent_output = use_agent_output_store();
    let notifications = use_notification_store();
    let mut mounted = use_signal(|| false);

    let unlistens: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    // One-time mount effect: register global Tauri event listeners.
    let unlistens_effect = unlistens.clone();
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        // Listener for agent:status
        let mut status_store = agent_status;
        let status_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("agent:status", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if pane_id.is_empty() {
                    return;
                }

                let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("idle");
                let status_enum = match status {
                    "thinking" => AgentRunStatus::Thinking,
                    "working" => AgentRunStatus::Working,
                    "waiting_for_input" => AgentRunStatus::WaitingForInput,
                    "completed" => AgentRunStatus::Completed,
                    "error" => AgentRunStatus::Error,
                    "cancelled" => AgentRunStatus::Cancelled,
                    "disconnected" => AgentRunStatus::Disconnected,
                    _ => AgentRunStatus::Idle,
                };

                let message = val
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let progress = val.get("progress").and_then(|p| {
                    let current = p.get("current").and_then(|v| v.as_u64())? as usize;
                    let total = p.get("total").and_then(|v| v.as_u64())? as usize;
                    let label = p
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(crate::stores::agent_status::AgentProgress {
                        current,
                        total,
                        label,
                    })
                });

                let now = js_sys::Date::now() as i64;
                status_store.write().update_status(
                    pane_id,
                    AgentStatusUpdate {
                        status: Some(status_enum),
                        message,
                        progress,
                    },
                    now,
                );
            }
        }) {
            status_unlistens.borrow_mut().push(u);
        }

        // Listener for terminal:exit
        let mut exit_store = agent_status;
        let exit_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("terminal:exit", move |payload: String| {
            let pane_id = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                val.get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                payload.trim_matches('"').to_string()
            };

            if !pane_id.is_empty() {
                let now = js_sys::Date::now() as i64;
                exit_store.write().update_status(
                    pane_id,
                    AgentStatusUpdate {
                        status: Some(AgentRunStatus::Disconnected),
                        message: Some("PTY exited".to_string()),
                        progress: None,
                    },
                    now,
                );
            }
        }) {
            exit_unlistens.borrow_mut().push(u);
        }

        // Listener for terminal:prompt
        let mut prompt_store = agent_status;
        let prompt_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("terminal:prompt", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !pane_id.is_empty() {
                    let now = js_sys::Date::now() as i64;
                    prompt_store.write().update_status(
                        pane_id,
                        AgentStatusUpdate {
                            status: Some(AgentRunStatus::Idle),
                            message: None,
                            progress: None,
                        },
                        now,
                    );
                }
            }
        }) {
            prompt_unlistens.borrow_mut().push(u);
        }

        // Listener for terminal:data
        let mut terminal_store = use_terminal_store();
        let terminal_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("terminal:data", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let session_id = val
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !session_id.is_empty() {
                    terminal_store.write().on_data(&session_id, &payload);
                }
            }
        }) {
            terminal_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:connected
        let mut connect_store = agent_status;
        let connect_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("agents:connected", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !pane_id.is_empty() {
                    let now = js_sys::Date::now() as i64;
                    connect_store.write().connect_agent(pane_id, now);
                }
            }
        }) {
            connect_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:disconnected
        let mut disconnect_store = agent_status;
        let disconnect_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("agents:disconnected", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !pane_id.is_empty() {
                    let now = js_sys::Date::now() as i64;
                    disconnect_store.write().disconnect_agent(&pane_id, now);
                }
            }
        }) {
            disconnect_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:statusUpdate
        let mut update_store = agent_status;
        let update_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("agents:statusUpdate", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if pane_id.is_empty() {
                    return;
                }
                let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("idle");
                let status_enum = match status {
                    "thinking" => AgentRunStatus::Thinking,
                    "working" => AgentRunStatus::Working,
                    "waiting_for_input" => AgentRunStatus::WaitingForInput,
                    "completed" => AgentRunStatus::Completed,
                    "error" => AgentRunStatus::Error,
                    "cancelled" => AgentRunStatus::Cancelled,
                    "disconnected" => AgentRunStatus::Disconnected,
                    _ => AgentRunStatus::Idle,
                };
                let message = val
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let now = js_sys::Date::now() as i64;
                update_store.write().update_status(
                    pane_id,
                    AgentStatusUpdate {
                        status: Some(status_enum),
                        message,
                        progress: None,
                    },
                    now,
                );
            }
        }) {
            update_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:inputRequested
        let mut input_store = agent_status;
        let mut input_notif_store = notifications;
        let input_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("agents:inputRequested", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let message = val
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Agent is requesting input")
                    .to_string();
                if !pane_id.is_empty() {
                    let now = js_sys::Date::now() as i64;
                    input_store
                        .write()
                        .request_input(pane_id.clone(), message.clone(), now);

                    // Also push a NeedsInput notification.
                    let notif = NotificationRecord {
                        id: format!("input-{}", chrono::Utc::now().timestamp_millis()),
                        r#type: NotificationType::NeedsInput,
                        title: "Agent Input Requested".to_string(),
                        message,
                        source: "agent".to_string(),
                        read: false,
                        timestamp: chrono::Utc::now().timestamp(),
                    };
                    add_notification(&mut input_notif_store, notif);
                }
            }
        }) {
            input_unlistens.borrow_mut().push(u);
        }

        // Listener for output-capture:line
        let mut output_store = agent_output;
        let output_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("output-capture:line", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let text = val
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let line_num = val.get("lineNum").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let timestamp = val.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);

                if !pane_id.is_empty() {
                    let line = crate::stores::agent_output::OutputLine {
                        pane_id,
                        line_num,
                        timestamp,
                        text,
                    };
                    output_store.write().append_line(line);
                }
            }
        }) {
            output_unlistens.borrow_mut().push(u);
        }

        // Listener for output-capture:paneRegistered
        let mut register_store = agent_output;
        let register_unlistens = unlistens_effect.clone();
        if let Ok(u) =
            tauri_bridge::listen("output-capture:paneRegistered", move |payload: String| {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                    let pane_id = val
                        .get("paneId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let agent_type = val
                        .get("agentType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if !pane_id.is_empty() {
                        let now = js_sys::Date::now() as i64;
                        register_store
                            .write()
                            .register_pane(pane_id, agent_type, now);
                    }
                }
            })
        {
            register_unlistens.borrow_mut().push(u);
        }

        // Listener for output-capture:paneUnregistered
        let mut unregister_store = agent_output;
        let unregister_unlistens = unlistens_effect.clone();
        if let Ok(u) =
            tauri_bridge::listen("output-capture:paneUnregistered", move |payload: String| {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                    let pane_id = val
                        .get("paneId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !pane_id.is_empty() {
                        unregister_store.write().unregister_pane(&pane_id);
                    }
                }
            })
        {
            unregister_unlistens.borrow_mut().push(u);
        }
    });

    // Cleanup: unlisten all event listeners on component unmount.
    let unlistens_drop = unlistens.clone();
    use_drop(move || {
        let handles = unlistens_drop.borrow_mut().drain(..).collect::<Vec<_>>();
        for handle in handles {
            handle();
        }
    });

    rsx! {}
}
