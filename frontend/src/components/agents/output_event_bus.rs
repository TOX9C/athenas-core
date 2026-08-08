use crate::stores::agent_output::{is_stderr_like, use_agent_output_store};
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus, AgentStatusUpdate};
use crate::stores::notification::{
    add_notification, use_notification_store, NotificationRecord, NotificationType,
};
use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge;
use crate::types::workspace::AgentType;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[path = "output_bus_event.rs"]
mod output_bus_event;
use output_bus_event::OutputBusEvent;

/// Output event bus component - renders nothing, handles IPC events.
///
/// Wires Tauri push events to the agent status and agent output stores:
/// 1. `agent:status:{pane_id}` - explicit status updates from the backend
/// 2. `terminal:exit:{pane_id}` - PTY exit transitions to Disconnected
/// 3. `agents:connected` - Add agent to status list
/// 4. `agents:disconnected` - Remove/update agent status
/// 5. `agents:statusUpdate` - Update agent status
/// 6. `agents:inputRequested` - Show input request
/// 7. `output-capture:batch` - Append batched output lines
/// 8. `output-capture:paneRegistered` - Register new pane
///
/// Also sets up heuristic shell-prompt detection: when terminal data arrives
/// containing a shell prompt pattern, the agent status transitions to Idle.
#[component]
pub fn OutputEventBus() -> Element {
    let agent_status = use_agent_status_store();
    let agent_output = use_agent_output_store();
    let notifications = use_notification_store();
    // `use_terminal_store()` and `use_terminal_registry()` are Dioxus hooks
    // (`use_context`). They may only run synchronously during render — calling
    // them inside the `use_coroutine` async body (which runs after render, on
    // every `TerminalData` event from the PTY) re-enters the hook list and
    // panics at mount with "hook list already borrowed". Capture both here and
    // move cheap clones into the coroutine below.
    let terminal_store = use_terminal_store();
    let terminal_registry = use_terminal_registry();
    // Consumed by the LLM title-summarization trigger folded in from the
    // removed `AgentInfoPoller`: `smart_pane_titles` gates the feature and
    // the workspace store tells us which panes are real agent panes (shells
    // must never summarize).
    let ui_state = use_ui_store();
    let workspace = use_workspace_store();
    let mut mounted = use_signal(|| false);

    let unlistens: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    // Dispatcher coroutine: receives parsed events from the Tauri listen
    // callbacks and performs all signal writes inside the reactive runtime.
    let dispatcher = use_coroutine(move |mut rx: UnboundedReceiver<OutputBusEvent>| {
        // Clone the non-`Copy` registry capture in the OUTER closure
        // scope (before `async move`), so the async block owns a fresh
        // clone and the `FnMut` outer closure never moves the render-top
        // capture out twice. `Signal`s are `Copy`, so the store/status
        // captures below don't need this treatment.
        let terminal_registry = terminal_registry.clone();
        async move {
            let mut agent_status = agent_status;
            let mut agent_output = agent_output;
            let mut notifications = notifications;
            let mut terminal_store = terminal_store;
            // (pane_id, session_id) pairs already LLM-summarized — moved here
            // from the removed `AgentInfoPoller` so a session change triggers
            // exactly one title call per pane per session.
            let mut summarized_pairs: HashSet<(String, String)> = HashSet::new();
            while let Ok(event) = rx.recv().await {
                match event {
                    OutputBusEvent::AgentStatus {
                        pane_id,
                        status,
                        message,
                        progress,
                        now,
                        fg_process,
                        task_title,
                        session_id,
                        raw_prompt,
                    } => {
                        agent_status.write().update_status(
                            &pane_id,
                            AgentStatusUpdate {
                                status: Some(status),
                                message,
                                progress,
                            },
                            now,
                        );

                        // Session change → reset the stale title even when
                        // summarization is disabled (an old Done title must
                        // never leak into the new session). Mirrors the
                        // `AgentInfoPoller` behavior. Must run BEFORE
                        // `update_agent_info` below, which writes the new
                        // session id into the registry.
                        let sid = session_id.clone().unwrap_or_default();
                        {
                            let old_sid = terminal_registry
                                .peek_session(&pane_id)
                                .and_then(|s| s.session_id);
                            if old_sid.as_deref() != Some(sid.as_str()) && !sid.is_empty() {
                                if let Some(mut inner) = terminal_registry.write_session(&pane_id) {
                                    inner.title_state = crate::utils::pane_label::TitleState::Idle;
                                    inner.generation = inner.generation.wrapping_add(1);
                                }
                            }
                        }

                        // Write the enriched fields into the per-pane terminal
                        // store so pane pills stay in sync WITHOUT the frontend
                        // polling `ps` itself (single-poller consolidation:
                        // the backend tracker is now the only process poller).
                        terminal_store.write().update_agent_info(
                            &terminal_registry,
                            &pane_id,
                            fg_process,
                            task_title,
                            session_id.clone(),
                            raw_prompt.clone(),
                        );

                        // LLM title summarization on session change (moved
                        // from `AgentInfoPoller`). Only real agent panes get
                        // summarized; shells must never scrape global state.
                        let raw = raw_prompt.unwrap_or_default();
                        let feature_enabled = ui_state.read().smart_pane_titles;
                        let prompt_ready = !sid.is_empty() && !raw.trim().is_empty();
                        let is_shell = workspace.read().spaces.iter().any(|s| {
                            s.panes.iter().any(|p| {
                                p.id == pane_id && matches!(p.agent_type, AgentType::Shell)
                            })
                        });
                        if feature_enabled && prompt_ready && !is_shell {
                            let key = (pane_id.clone(), sid.clone());
                            if !summarized_pairs.contains(&key) {
                                summarized_pairs.insert(key);
                                // Mark the title as pending in a single write
                                // (the pill shows Pending while the LLM call
                                // is in flight, then Done/Failed).
                                if let Some(mut inner) = terminal_registry.write_session(&pane_id) {
                                    inner.title_state =
                                        crate::utils::pane_label::TitleState::Pending;
                                    inner.generation = inner.generation.wrapping_add(1);
                                }
                                let registry_for_spawn = terminal_registry.clone();
                                let raw_prompt = raw.clone();
                                let pane = pane_id.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    let result =
                                        crate::tauri_bridge::summarize_agent_title(&raw_prompt)
                                            .await;
                                    let Some(mut inner) = registry_for_spawn.write_session(&pane)
                                    else {
                                        return;
                                    };
                                    match result {
                                        Ok(summary) => {
                                            let cleaned = summary.trim().to_string();
                                            web_sys::console::log_1(
                                                &format!(
                                                    "[OutputEventBus] title for pane={}: {}",
                                                    pane, cleaned
                                                )
                                                .into(),
                                            );
                                            inner.title_state =
                                                crate::utils::pane_label::TitleState::Done(cleaned);
                                        }
                                        Err(e) => {
                                            web_sys::console::warn_1(
                                                &format!(
                                                    "[OutputEventBus] title failed for pane={}: {:?}",
                                                    pane, e
                                                )
                                                .into(),
                                            );
                                            inner.title_state =
                                                crate::utils::pane_label::TitleState::Failed;
                                        }
                                    }
                                    inner.generation = inner.generation.wrapping_add(1);
                                });
                            }
                        }
                    }
                    OutputBusEvent::TerminalExit { pane_id, now } => {
                        agent_status.write().update_status(
                            pane_id,
                            AgentStatusUpdate {
                                status: Some(AgentRunStatus::Disconnected),
                                message: Some("PTY exited".to_string()),
                                progress: None,
                            },
                            now,
                        );
                    }
                    OutputBusEvent::TerminalData {
                        session_id,
                        payload,
                    } => {
                        terminal_store
                            .write()
                            .on_data(&terminal_registry, &session_id, &payload);
                    }
                    OutputBusEvent::AgentConnected { pane_id, now } => {
                        agent_status.write().connect_agent(pane_id, now);
                    }
                    OutputBusEvent::AgentDisconnected { pane_id, now } => {
                        agent_status.write().disconnect_agent(&pane_id, now);
                    }
                    OutputBusEvent::AgentStatusUpdate {
                        pane_id,
                        status,
                        message,
                        now,
                    } => {
                        agent_status.write().update_status(
                            pane_id,
                            AgentStatusUpdate {
                                status: Some(status),
                                message,
                                progress: None,
                            },
                            now,
                        );
                    }
                    OutputBusEvent::InputRequested {
                        pane_id,
                        message,
                        now,
                    } => {
                        agent_status
                            .write()
                            .request_input(pane_id.clone(), message.clone(), now);

                        let notif = NotificationRecord {
                            id: format!("input-{}", chrono::Utc::now().timestamp_millis()),
                            r#type: NotificationType::NeedsInput,
                            title: "Agent Input Requested".to_string(),
                            message,
                            source: "agent".to_string(),
                            agent_id: Some(pane_id.clone()),
                            data: Some(serde_json::json!({ "paneId": pane_id })),
                            metadata: None,
                            actions: None,
                            request_id: None,
                            dismissed_at: None,
                            read: false,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            count: 1,
                        };
                        add_notification(&mut notifications, notif);
                    }
                    OutputBusEvent::OutputLine(line) => {
                        agent_output.write().append_line(line);
                    }
                    OutputBusEvent::PaneRegistered {
                        pane_id,
                        agent_type,
                        now,
                    } => {
                        agent_output.write().register_pane(pane_id, agent_type, now);
                    }
                }
            }
        }
    });

    // One-time mount effect: register global Tauri event listeners.
    let unlistens_effect = unlistens.clone();
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        // Listener for agent:status

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
                    "waiting_for_input" | "waiting_input" => AgentRunStatus::WaitingForInput,
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

                // Enriched fields (single-poller consolidation): the backend
                // tracker's heartbeat carries the raw foreground label + the
                // scraped session metadata so the frontend never polls `ps`.
                let fg_process = val
                    .get("fgProcess")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let task_title = val
                    .get("taskTitle")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let session_id = val
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let raw_prompt = val
                    .get("rawPrompt")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());

                let now = js_sys::Date::now() as i64;
                dispatcher.send(OutputBusEvent::AgentStatus {
                    pane_id,
                    status: status_enum,
                    message,
                    progress,
                    now,
                    fg_process,
                    task_title,
                    session_id,
                    raw_prompt,
                });
            }
        }) {
            status_unlistens.borrow_mut().push(u);
        }

        // Listener for terminal:exit

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
                dispatcher.send(OutputBusEvent::TerminalExit { pane_id, now });
            }
        }) {
            exit_unlistens.borrow_mut().push(u);
        }

        // Listener for terminal:data

        let terminal_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("terminal:data", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let session_id = val
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !session_id.is_empty() {
                    dispatcher.send(OutputBusEvent::TerminalData {
                        session_id,
                        payload,
                    });
                }
            }
        }) {
            terminal_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:connected

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
                    dispatcher.send(OutputBusEvent::AgentConnected { pane_id, now });
                }
            }
        }) {
            connect_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:disconnected

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
                    dispatcher.send(OutputBusEvent::AgentDisconnected { pane_id, now });
                }
            }
        }) {
            disconnect_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:statusUpdate

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
                    "waiting_for_input" | "waiting_input" => AgentRunStatus::WaitingForInput,
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
                dispatcher.send(OutputBusEvent::AgentStatusUpdate {
                    pane_id,
                    status: status_enum,
                    message,
                    now,
                });
            }
        }) {
            update_unlistens.borrow_mut().push(u);
        }

        // Listener for agents:inputRequested

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
                    .or_else(|| val.get("prompt"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Agent is requesting input")
                    .to_string();
                if !pane_id.is_empty() {
                    let now = js_sys::Date::now() as i64;
                    dispatcher.send(OutputBusEvent::InputRequested {
                        pane_id,
                        message,
                        now,
                    });
                }
            }
        }) {
            input_unlistens.borrow_mut().push(u);
        }

        // Listener for output-capture:batch (replaces per-line emission)

        let batch_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("output-capture:batch", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let pane_id = val
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(lines) = val.get("lines").and_then(|v| v.as_array()) {
                    for line_val in lines {
                        let text = line_val
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let line_num = line_val
                            .get("lineNum")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        let timestamp = line_val
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        if !pane_id.is_empty() {
                            let is_stderr = is_stderr_like(&text);
                            let line = crate::stores::agent_output::OutputLine {
                                pane_id: pane_id.clone(),
                                line_num,
                                timestamp,
                                text,
                                is_stderr,
                            };
                            dispatcher.send(OutputBusEvent::OutputLine(line));
                        }
                    }
                }
            }
        }) {
            batch_unlistens.borrow_mut().push(u);
        }

        // Listener for output-capture:paneRegistered

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
                        dispatcher.send(OutputBusEvent::PaneRegistered {
                            pane_id,
                            agent_type,
                            now,
                        });
                    }
                }
            })
        {
            register_unlistens.borrow_mut().push(u);
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
