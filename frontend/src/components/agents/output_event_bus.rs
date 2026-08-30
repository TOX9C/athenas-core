use crate::stores::agent_output::{is_stderr_like, use_agent_output_store};
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus, AgentStatusUpdate};
use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge;
use crate::types::workspace::AgentType;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[path = "output_bus_event.rs"]
mod output_bus_event;
use output_bus_event::OutputBusEvent;

#[path = "swarm_status_sync.rs"]
mod swarm_status_sync;
use swarm_status_sync::{PaneGenerationGuard, SwarmStatusSync, SwarmStatusUpdate};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TitleRequestKey {
    session_id: String,
    generation: Option<u64>,
}

#[derive(Debug, Default)]
struct TitleRequestTracker {
    latest_by_pane: HashMap<String, TitleRequestKey>,
    started: HashSet<(String, TitleRequestKey)>,
}

impl TitleRequestTracker {
    fn observe(&mut self, pane_id: &str, request: TitleRequestKey) {
        self.latest_by_pane.insert(pane_id.to_string(), request);
    }

    fn begin(&mut self, pane_id: &str, request: &TitleRequestKey) -> bool {
        self.started.insert((pane_id.to_string(), request.clone()))
    }

    fn invalidate(&mut self, pane_id: &str) {
        if let Some(request) = self.latest_by_pane.remove(pane_id) {
            self.started.remove(&(pane_id.to_string(), request));
        }
    }

    fn accepts_result(
        &self,
        pane_id: &str,
        request: &TitleRequestKey,
        current_session_id: Option<&str>,
    ) -> bool {
        self.latest_by_pane.get(pane_id) == Some(request)
            && current_session_id == Some(request.session_id.as_str())
    }
}

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
    let swarm_state = crate::stores::swarm::use_swarm_store();
    let mut mounted = use_signal(|| false);

    let unlistens: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    // Serialize durable swarm writes so a slower earlier IPC call cannot finish
    // after a newer status and leave the persisted state stale.
    let swarm_writer = use_coroutine(|mut rx: UnboundedReceiver<SwarmStatusUpdate>| async move {
        while let Ok(update) = rx.recv().await {
            let _ = crate::tauri_bridge::swarm_update_agent(
                &update.dir,
                &update.agent_id,
                Some(update.status),
                update.last_action.as_deref(),
                None,
            )
            .await;
        }
    });

    // Dispatcher coroutine: receives parsed events from the Tauri listen
    // callbacks and performs all signal writes inside the reactive runtime.
    let dispatcher = use_coroutine(move |mut rx: UnboundedReceiver<OutputBusEvent>| {
        // Clone the registry capture in the OUTER closure scope (before
        // `async move`), so the async block owns a fresh clone and the `FnMut`
        // outer closure never moves the render-top capture out twice.
        let terminal_registry = terminal_registry.clone();
        let swarm_writer = swarm_writer;
        async move {
            let mut agent_status = agent_status;
            let mut agent_output = agent_output;
            let mut terminal_store = terminal_store;
            let swarm_state = swarm_state;
            // Tracks the latest title request identity per pane. The tracker
            // is shared with spawned LLM tasks so a late result can verify it
            // still belongs to the current session and backend generation.
            let title_requests = Rc::new(RefCell::new(TitleRequestTracker::default()));
            let mut swarm_sync = SwarmStatusSync::default();
            let mut pane_generation_guard = PaneGenerationGuard::default();
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
                        generation,
                    } => {
                        if !pane_generation_guard.accepts(&pane_id, generation) {
                            continue;
                        }
                        let stale_generation = generation.is_some_and(|incoming| {
                            agent_status
                                .read()
                                .statuses
                                .iter()
                                .find(|(id, _)| id == &pane_id)
                                .and_then(|(_, current)| current.generation)
                                .is_some_and(|current| current != incoming)
                        });
                        if stale_generation {
                            continue;
                        }
                        // Reopen only after the event has passed both the
                        // retired-pane guard and the currently-live generation
                        // check. A rejected intermediate-generation event must
                        // never clear the tombstone for a later generationless
                        // stale event.
                        if generation.is_some() {
                            pane_generation_guard.reopen(&pane_id);
                        }

                        agent_status.write().update_status(
                            &pane_id,
                            AgentStatusUpdate {
                                generation,
                                status: Some(status.clone()),
                                message: message.clone(),
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
                        let title_request = TitleRequestKey {
                            session_id: sid.clone(),
                            generation,
                        };
                        title_requests
                            .borrow_mut()
                            .observe(&pane_id, title_request.clone());
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

                        // Persist live status for panes that belong to the
                        // active mission. The backend maps by pane id, so a
                        // status event from an unrelated terminal is ignored
                        // without an extra mission-specific event channel.
                        let swarm_dir = {
                            let state = workspace.read();
                            state.active_space_id.as_ref().and_then(|id| {
                                state
                                    .spaces
                                    .iter()
                                    .find(|space| &space.id == id)
                                    .map(|space| space.dir.clone())
                            })
                        };
                        let swarm_agent_id =
                            swarm_state.read().active_swarm.as_ref().and_then(|swarm| {
                                swarm
                                    .agents
                                    .iter()
                                    .find(|agent| agent.pane_id == pane_id)
                                    .map(|agent| agent.id.clone())
                            });
                        if let (Some(dir), Some(agent_id)) = (swarm_dir, swarm_agent_id) {
                            let persisted_status = match status {
                                AgentRunStatus::Thinking => "thinking",
                                AgentRunStatus::Working => "writing",
                                AgentRunStatus::WaitingForInput => "waiting",
                                AgentRunStatus::Completed => "done",
                                AgentRunStatus::Error => "blocked",
                                AgentRunStatus::Disconnected => "stalled",
                                _ => "idle",
                            };
                            let update = SwarmStatusUpdate {
                                dir,
                                agent_id,
                                status: persisted_status,
                                last_action: message.clone(),
                            };
                            if swarm_sync.should_send(&pane_id, update.clone(), now) {
                                swarm_writer.send(update);
                            }
                        }

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
                        if feature_enabled
                            && prompt_ready
                            && !is_shell
                            && title_requests.borrow_mut().begin(&pane_id, &title_request)
                        {
                            // Mark the title as pending in a single write
                            // (the pill shows Pending while the LLM call
                            // is in flight, then Done/Failed).
                            if let Some(mut inner) = terminal_registry.write_session(&pane_id) {
                                inner.title_state = crate::utils::pane_label::TitleState::Pending;
                                inner.generation = inner.generation.wrapping_add(1);
                            }
                            let registry_for_spawn = terminal_registry.clone();
                            let raw_prompt = raw.clone();
                            let pane = pane_id.clone();
                            let title_requests_for_spawn = title_requests.clone();
                            let title_request_for_spawn = title_request.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                let result =
                                    crate::tauri_bridge::summarize_agent_title(&raw_prompt).await;
                                let current_session_id = registry_for_spawn
                                    .peek_session(&pane)
                                    .and_then(|session| session.session_id);
                                if !title_requests_for_spawn.borrow().accepts_result(
                                    &pane,
                                    &title_request_for_spawn,
                                    current_session_id.as_deref(),
                                ) {
                                    return;
                                }
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
                    OutputBusEvent::TerminalExit {
                        pane_id,
                        generation,
                    } => {
                        // `terminal:exit` is the authoritative lifecycle
                        // boundary. Ignore an old PTY's exit after a pane id
                        // has already been reused by a newer generation.
                        let is_current = agent_status
                            .read()
                            .statuses
                            .iter()
                            .find(|(id, _)| id == &pane_id)
                            .map(|(_, status)| match (generation, status.generation) {
                                // A generation-bearing exit is safe to apply
                                // only when the pane still has that exact PTY
                                // generation. Never let a late old exit stall
                                // a newly reused pane.
                                (Some(event_generation), Some(current_generation)) => {
                                    event_generation == current_generation
                                }
                                (Some(_), None) => false,
                                // Legacy exit events without a generation can
                                // still retire the current pane status.
                                (None, _) => true,
                            })
                            .unwrap_or(generation.is_none());
                        if is_current {
                            agent_status.write().remove_status(&pane_id);
                        }
                        if !is_current {
                            continue;
                        }
                        title_requests.borrow_mut().invalidate(&pane_id);
                        pane_generation_guard.retire(&pane_id, generation);
                        swarm_sync.remove(&pane_id);
                        let swarm_dir = {
                            let state = workspace.read();
                            state.active_space_id.as_ref().and_then(|id| {
                                state
                                    .spaces
                                    .iter()
                                    .find(|space| &space.id == id)
                                    .map(|space| space.dir.clone())
                            })
                        };
                        let swarm_agent_id =
                            swarm_state.read().active_swarm.as_ref().and_then(|swarm| {
                                swarm
                                    .agents
                                    .iter()
                                    .find(|agent| agent.pane_id == pane_id)
                                    .map(|agent| agent.id.clone())
                            });
                        if let (Some(dir), Some(agent_id)) = (swarm_dir, swarm_agent_id) {
                            swarm_writer.send(SwarmStatusUpdate {
                                dir,
                                agent_id,
                                status: "stalled",
                                last_action: Some("Terminal exited".to_string()),
                            });
                        }
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
                        // A generationless connection cannot prove that it is
                        // newer than a retired PTY. Do not let a delayed
                        // connection event clear the tombstone or recreate a
                        // status entry for a pane that has already exited.
                        if !pane_generation_guard.accepts(&pane_id, None) {
                            continue;
                        }
                        agent_status.write().connect_agent(pane_id, now);
                    }
                    OutputBusEvent::AgentDisconnected { pane_id, now } => {
                        title_requests.borrow_mut().invalidate(&pane_id);
                        pane_generation_guard.retire(&pane_id, None);
                        agent_status.write().disconnect_agent(&pane_id, now);
                        swarm_sync.remove(&pane_id);
                        let swarm_dir = {
                            let state = workspace.read();
                            state.active_space_id.as_ref().and_then(|id| {
                                state
                                    .spaces
                                    .iter()
                                    .find(|space| &space.id == id)
                                    .map(|space| space.dir.clone())
                            })
                        };
                        let swarm_agent_id =
                            swarm_state.read().active_swarm.as_ref().and_then(|swarm| {
                                swarm
                                    .agents
                                    .iter()
                                    .find(|agent| agent.pane_id == pane_id)
                                    .map(|agent| agent.id.clone())
                            });
                        if let (Some(dir), Some(agent_id)) = (swarm_dir, swarm_agent_id) {
                            swarm_writer.send(SwarmStatusUpdate {
                                dir,
                                agent_id,
                                status: "stalled",
                                last_action: Some("Agent disconnected".to_string()),
                            });
                        }
                    }
                    OutputBusEvent::AgentStatusUpdate {
                        pane_id,
                        status,
                        message,
                        now,
                    } => {
                        if !pane_generation_guard.accepts(&pane_id, None) {
                            continue;
                        }
                        agent_status.write().update_status(
                            &pane_id,
                            AgentStatusUpdate {
                                generation: None,
                                status: Some(status.clone()),
                                message: message.clone(),
                                progress: None,
                            },
                            now,
                        );
                        let persisted_status = match status {
                            AgentRunStatus::Thinking => "thinking",
                            AgentRunStatus::Working => "writing",
                            AgentRunStatus::WaitingForInput => "waiting",
                            AgentRunStatus::Completed => "done",
                            AgentRunStatus::Error => "blocked",
                            AgentRunStatus::Disconnected => "stalled",
                            _ => "idle",
                        };
                        let swarm_dir = {
                            let state = workspace.read();
                            state.active_space_id.as_ref().and_then(|id| {
                                state
                                    .spaces
                                    .iter()
                                    .find(|space| &space.id == id)
                                    .map(|space| space.dir.clone())
                            })
                        };
                        let swarm_agent_id =
                            swarm_state.read().active_swarm.as_ref().and_then(|swarm| {
                                swarm
                                    .agents
                                    .iter()
                                    .find(|agent| agent.pane_id == pane_id)
                                    .map(|agent| agent.id.clone())
                            });
                        if let (Some(dir), Some(agent_id)) = (swarm_dir, swarm_agent_id) {
                            let update = SwarmStatusUpdate {
                                dir,
                                agent_id,
                                status: persisted_status,
                                last_action: message.clone(),
                            };
                            if swarm_sync.should_send(&pane_id, update.clone(), now) {
                                swarm_writer.send(update);
                            }
                        }
                    }
                    OutputBusEvent::InputRequested {
                        pane_id,
                        request_id: _request_id,
                        message,
                        now,
                    } => {
                        // The backend owns the durable NeedsInput record and
                        // emits notifications:new. This bus only updates the
                        // live agent status; creating a second frontend-only
                        // record caused missing persistence and duplicate rows.
                        agent_status.write().request_input(pane_id, message, now);
                    }
                    OutputBusEvent::OutputBatch { pane_id, lines } => {
                        agent_output.write().append_batch(&pane_id, lines);
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
                let generation = val.get("generation").and_then(|v| v.as_u64());
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
                    generation,
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
                let generation = serde_json::from_str::<serde_json::Value>(&payload)
                    .ok()
                    .and_then(|value| value.get("generation").and_then(|v| v.as_u64()));
                dispatcher.send(OutputBusEvent::TerminalExit {
                    pane_id,
                    generation,
                });
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
                let request_id = val
                    .get("requestId")
                    .or_else(|| val.get("request_id"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
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
                        request_id,
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
                if !pane_id.is_empty() {
                    let batch = val
                        .get("lines")
                        .and_then(|v| v.as_array())
                        .map(|lines| {
                            lines
                                .iter()
                                .map(|line_val| {
                                    let text = line_val
                                        .get("text")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    crate::stores::agent_output::OutputLine {
                                        pane_id: pane_id.clone(),
                                        line_num: line_val
                                            .get("lineNum")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            as usize,
                                        timestamp: line_val
                                            .get("timestamp")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0),
                                        is_stderr: is_stderr_like(&text),
                                        text,
                                    }
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    if !batch.is_empty() {
                        dispatcher.send(OutputBusEvent::OutputBatch {
                            pane_id,
                            lines: batch,
                        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_results_require_the_latest_session_and_generation() {
        let mut tracker = TitleRequestTracker::default();
        let first = TitleRequestKey {
            session_id: "session-a".to_string(),
            generation: Some(1),
        };
        let newer_generation = TitleRequestKey {
            session_id: "session-a".to_string(),
            generation: Some(2),
        };
        let newer_session = TitleRequestKey {
            session_id: "session-b".to_string(),
            generation: Some(3),
        };

        tracker.observe("pane-1", first.clone());
        assert!(tracker.begin("pane-1", &first));
        assert!(tracker.accepts_result("pane-1", &first, Some("session-a")));

        tracker.observe("pane-1", newer_generation.clone());
        assert!(!tracker.accepts_result("pane-1", &first, Some("session-a")));
        assert!(!tracker.accepts_result("pane-1", &newer_generation, Some("session-b")));

        tracker.observe("pane-1", newer_session.clone());
        assert!(!tracker.accepts_result("pane-1", &newer_generation, Some("session-a")));
        assert!(tracker.accepts_result("pane-1", &newer_session, Some("session-b")));

        tracker.invalidate("pane-1");
        assert!(!tracker.accepts_result("pane-1", &newer_session, Some("session-b")));
    }
}
