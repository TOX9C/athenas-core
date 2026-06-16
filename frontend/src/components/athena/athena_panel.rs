use super::athena_input::AthenaInput;
use super::chat_message::AthenaChatMessage;
use super::session_switcher::SessionSwitcher;
use super::thinking::AthenaThinkingIndicator;
use crate::stores::athena::{
    use_athena_store, AskUserOption, AthenaMessage, MessageRole, PlanStatus, PlanStepStatus,
    StepEvaluation,
};
use crate::components::shared::icon::IconClose;
use crate::components::shared::illustration::OwlMark;
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Rendering mode for the Athena chat panel.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum AthenaPanelMode {
    /// Slide-up overlay fixed to the bottom of the viewport.
    #[default]
    Overlay,
    /// Contained panel that fills its parent's flex container.
    Compact,
}

#[derive(Props, Clone, PartialEq)]
pub struct AthenaPanelProps {
    #[props(default = AthenaPanelMode::Overlay)]
    pub mode: AthenaPanelMode,
}

#[component]
pub fn AthenaPanel(props: AthenaPanelProps) -> Element {
    let mut athena_state = use_athena_store();
    let mut mounted = use_signal(|| false);
    let unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let unlisteners_clone = unlisteners.clone();

    let is_open = athena_state.read().is_open;
    let mode = props.mode;

    // Only gate visibility by is_open in overlay mode.
    // In compact mode the parent (right sidebar) controls visibility.
    if mode == AthenaPanelMode::Overlay && !is_open {
        return rsx! {};
    }

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        let store = athena_state;

        // athena:status — Update thinking/working/idle state.
        let mut status_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:status", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("idle");
                let detail = val
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                status_store.write().handle_status_event(status, detail);
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // athena:askUser — Show interactive user question modal.
        let mut ask_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:askUser", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let request_id = val
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let question = val
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let options: Vec<AskUserOption> = val
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|o| {
                                let label = o.get("label").and_then(|v| v.as_str())?.to_string();
                                let description = o
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Some(AskUserOption { label, description })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ask_store
                    .write()
                    .handle_ask_user(request_id, question, options);
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // athena:planUpdate — Update plan display.
        let mut plan_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:planUpdate", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let plan_id = val
                    .get("planId")
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
                    .unwrap_or("pending");
                let status = match status_str {
                    "in_progress" => PlanStatus::InProgress,
                    "completed" => PlanStatus::Completed,
                    "failed" => PlanStatus::Failed,
                    _ => PlanStatus::Pending,
                };
                let steps: Vec<_> = val
                    .get("steps")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| {
                                let id = s.get("id").and_then(|v| v.as_str())?.to_string();
                                let title = s.get("title").and_then(|v| v.as_str())?.to_string();
                                let description = s
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let agent_type = s
                                    .get("agentType")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let step_status_str = s
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("pending");
                                let step_status = match step_status_str {
                                    "in_progress" => PlanStepStatus::InProgress,
                                    "completed" => PlanStepStatus::Completed,
                                    "failed" => PlanStepStatus::Failed,
                                    _ => PlanStepStatus::Pending,
                                };
                                Some(crate::stores::athena::PlanStepBlock {
                                    id,
                                    title,
                                    description,
                                    agent_type,
                                    status: step_status,
                                    assigned_pane_id: s
                                        .get("assignedPaneId")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    result_summary: s
                                        .get("resultSummary")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                plan_store
                    .write()
                    .handle_plan_update(plan_id, goal, steps, status);
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // athena:planEvaluated — Show evaluation results.
        let mut eval_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:planEvaluated", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let plan_id = val
                    .get("planId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let overall_status = val
                    .get("overallStatus")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let next_action = val
                    .get("nextAction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let reasoning = val
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let step_evaluations: Vec<StepEvaluation> = val
                    .get("stepEvaluations")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| {
                                let step_id = s.get("stepId").and_then(|v| v.as_str())?.to_string();
                                let status = s.get("status").and_then(|v| v.as_str())?.to_string();
                                let summary = s
                                    .get("summary")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Some(StepEvaluation {
                                    step_id,
                                    status,
                                    summary,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                eval_store.write().handle_plan_evaluated(
                    plan_id,
                    overall_status,
                    step_evaluations,
                    next_action,
                    reasoning,
                );
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }
    });

    // Load most recent session on mount (restoration on restart).
    use_effect(move || {
        let mut athena = athena_state;
        spawn(async move {
            // Probe the backend for the real LLM configuration state BEFORE
            // doing anything else, so the panel renders with an accurate
            // "is the API configured?" indicator and the correct model label
            // instead of the stale in-memory defaults. `llm.api_key` is the
            // keyring-backed sentinel ("set" / "not_set"); `llm.model` is the
            // user-entered model string.
            match tauri_bridge::store_get("llm.api_key").await {
                Ok(v) => {
                    athena.write().set_api_configured(Some(v == "set"));
                    // Clear any previous keyring error on success
                    athena.write().set_api_keyring_error(None);
                }
                // If the keyring read failed (e.g. keychain locked), DON'T
                // flip the UI to "not set". Leave it at its previous value
                // (None or whatever was last known good) so the send button
                // stays usable and the user can at least *try* to send.
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[AthenaPanel] Keyring probe failed: {:?}. Leaving api_configured as-is.", e).into(),
                    );
                    athena.write().set_api_keyring_error(Some(format!("Keychain access failed: {:?}", e)));
                }
            }
            match tauri_bridge::store_get("llm.model").await {
                Ok(m) if !m.is_empty() => athena.write().set_configured_model(Some(m)),
                _ => athena.write().set_configured_model(None),
            }

            match tauri_bridge::session_list().await {
                Ok(json) => {
                    if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                        if let Some(session) = parsed.first() {
                            if let Some(id) = session.get("id").and_then(|v| v.as_str()) {
                                if let Ok(session_json) = tauri_bridge::session_get(id).await {
                                    if let Ok(val) =
                                        serde_json::from_str::<serde_json::Value>(&session_json)
                                    {
                                        if let Some(messages) =
                                            val.get("messages").and_then(|v| v.as_array())
                                        {
                                            let loaded: Vec<AthenaMessage> = messages
                                                .iter()
                                                .filter_map(|m| {
                                                    let role_str = m.get("role")?.as_str()?;
                                                    let role = if role_str.eq("user") {
                                                        MessageRole::User
                                                    } else {
                                                        MessageRole::Athena
                                                    };
                                                    let content =
                                                        m.get("content")?.as_str()?.to_string();
                                                    let id_val = m
                                                        .get("id")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or_default()
                                                        .to_string();
                                                    let timestamp = m
                                                        .get("timestamp")
                                                        .and_then(|v| v.as_u64())
                                                        .unwrap_or_else(|| {
                                                            chrono::Utc::now().timestamp() as u64
                                                        })
                                                        as i64;
                                                    let is_error = m
                                                        .get("isError")
                                                        .and_then(|v| v.as_bool())
                                                        .unwrap_or(false);
                                                    Some(AthenaMessage {
                                                        id: id_val,
                                                        role,
                                                        content,
                                                        timestamp,
                                                        is_error,
                                                        images: Vec::new(),
                                                        blocks: Vec::new(),
                                                    })
                                                })
                                                .collect();
                                            athena.write().set_messages(loaded);
                                            athena.write().set_session_id(Some(id.to_string()));
                                            let title = session
                                                .get("title")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("New Chat")
                                                .to_string();
                                            athena.write().set_session_title(title);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[AthenaPanel] Failed to load sessions: {:?}", e).into(),
                    );
                }
            }
        });
    });

    // Cleanup: unlisten all event listeners on component unmount.
    let unlisteners_drop = unlisteners.clone();
    use_drop(move || {
        for unlisten in unlisteners_drop.borrow_mut().drain(..) {
            unlisten();
        }
    });

    let state = athena_state.read();

    // Prefer the model actually persisted in the store over the stale
    // in-memory default. Falls back to "claude" only when nothing is
    // configured (matches the original display behaviour).
    let model_label = state
        .configured_model
        .clone()
        .filter(|m| !m.is_empty())
        .or_else(|| {
            if state.model.is_empty() {
                None
            } else {
                Some(state.model.clone())
            }
        })
        .unwrap_or_else(|| "claude".to_string());

    // A small status dot in the header: green when an API key is set,
    // amber when confirmed unset, red if the keyring is inaccessible,
    // neutral while still probing on mount.
    let (status_color, status_title) = match state.api_configured {
        Some(true) => {
            if state.api_keyring_error.is_some() {
                ("var(--warning)", "API key set but keychain locked")
            } else {
                ("var(--success)", "API keyconfigured")
            }
        }
        Some(false) => ("var(--warning)", "API key not set — configure in Settings"),
        None => ("var(--textDim)", "Checking configuration…"),
    };

    let wrapper_style = match mode {
        AthenaPanelMode::Overlay => "position: absolute; bottom: 0; left: 0; right: 0; height: 35vh; display: flex; flex-direction: row; background: var(--bg); color: var(--text); border-top: 1px solid var(--border); z-index: 100;",
        AthenaPanelMode::Compact => "flex: 1; display: flex; flex-direction: row; min-width: 0; min-height: 0; background: var(--bg); color: var(--text); overflow: hidden;",
    };

    rsx! {
        div {
            class: "athena-panel",
            style: wrapper_style,

            // Main chat area
            div {
                style: "flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0;",

                // Header
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                    span {
                        style: "font-family: var(--font-display); font-size: 17px; font-weight: 600; letter-spacing: 0.01em; color: var(--text); flex-shrink: 0;",
                        "Athena"
                    }

                    // Session switcher dropdown
                    SessionSwitcher {}

                    span {
                        class: "badge",
                        style: "color: var(--accent);",
                        "{model_label}"
                    }

                    // Configuration status dot — reflects the backend's
                    // keyring-backed probe, not the in-memory defaults.
                    span {
                        title: "{status_title}",
                        style: "width: 8px; height: 8px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                    }

                    if state.is_streaming {
                        span {
                            style: "font-size: var(--text-2xs); color: var(--accent); letter-spacing: 0.04em; text-transform: lowercase;",
                            "streaming..."
                        }
                    }
                }

                // Keyring failure warning — shows when the keychain is locked
                // but we have the confirmation flag, so the user knows why the
                // status dot may look odd.
                if let Some(ref err) = state.api_keyring_error {
                    div {
                        style: "display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--warning); background: rgba(235, 145, 19, 0.08); color: var(--warning); font-size: 12px;",
                        span { style: "flex-shrink: 0;", "⚠️" }
                        span { "{err}" }
                    }
                }

                // Pinned context bar
                if !state.dropped_context.is_empty() {
                    div {
                        style: "background: var(--bgTertiary); border-bottom: 1px solid var(--border); padding: 4px 12px; font-size: 11px; color: var(--textMuted); display: flex; flex-wrap: wrap; gap: 4px; align-items: center;",
                        span { style: "font-weight: 600; color: var(--accent);", "Context:" }
                        for (i, item) in state.dropped_context.iter().enumerate() {
                            {
                                let display = match item {
                                    crate::stores::athena::DraggableItem::Agent { pane_id, label, .. } => format!("Agent: {}", label),
                                    crate::stores::athena::DraggableItem::KanbanTask { title, .. } => format!("Task: {}", title),
                                    crate::stores::athena::DraggableItem::File { name, .. } => format!("File: {}", name),
                                };
                                rsx! {
                                    span {
                                        key: "context-{i}",
                                        style: "padding: 1px 6px; border-radius: 4px; background: var(--bg); border: 1px solid var(--border); font-size: 10px;",
                                        "{display}"
                                    }
                                }
                            }
                        }
                        button {
                            class: "icon-btn",
                            style: "margin-left: 4px;",
                            title: "Clear context",
                            onclick: move |_| {
                                let mut athena = athena_state.write();
                                athena.dropped_context.clear();
                            },
                            IconClose { size: Some(14), color: Some("currentColor".to_string()) }
                        }
                    }
                }

                // Messages
                div {
                    style: "flex: 1; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 8px;",

                    if state.messages.is_empty() {
                        div {
                            style: "flex: 1; display: flex; align-items: center; justify-content: center; color: var(--textDim);",
                            div {
                                style: "text-align: center; display: flex; flex-direction: column; align-items: center; gap: 10px;",
                                span {
                                    style: "opacity: 0.55; display: block;",
                                    OwlMark { size: Some(40) }
                                }
                                span { style: "font-family: var(--font-display); font-size: 15px;", "Ask Athena anything..." }
                            }
                        }
                    } else {
                        for msg in state.messages.iter() {
                            AthenaChatMessage { key: "{msg.id}", message: msg.clone() }
                        }
                    }

                    if state.is_streaming {
                        AthenaThinkingIndicator { status: state.streaming_status.clone() }
                    }
                }

                // Input
                AthenaInput {}
            }
        }
    }
}
