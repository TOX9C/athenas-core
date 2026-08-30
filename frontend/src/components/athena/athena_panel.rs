use super::athena_input::{submit_message_text, AthenaInput};
use super::chat_message::AthenaChatMessage;
use super::session_switcher::SessionSwitcher;
use super::thinking::AthenaThinkingIndicator;
use crate::components::shared::icon::{IconClose, IconWarning};
use crate::components::shared::illustration::CoreMark;
use crate::stores::athena::{
    use_athena_store, AskUserOption, AthenaMessage, MessageRole, PlanStatus, PlanStepStatus,
    StepEvaluation,
};
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

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

/// Coalesce streamed text deltas to one signal write per animation frame.
/// Provider chunks often arrive faster than the browser can paint; writing the
/// whole Athena state for each chunk needlessly re-renders the chat history.
fn schedule_stream_flush(
    mut store: Signal<crate::stores::athena::AthenaState>,
    queue: Rc<RefCell<Vec<(String, String)>>>,
    scheduled: Rc<RefCell<bool>>,
) {
    if *scheduled.borrow() {
        return;
    }
    *scheduled.borrow_mut() = true;

    let queue_for_frame = queue.clone();
    let scheduled_for_frame = scheduled.clone();
    let flush = move || {
        let deltas = queue_for_frame.borrow_mut().drain(..).collect::<Vec<_>>();
        if !deltas.is_empty() {
            let mut state = store.write();
            for (request_id, text) in deltas {
                state.append_stream_delta(&request_id, &text);
            }
        }
        *scheduled_for_frame.borrow_mut() = false;
    };
    let frame = wasm_bindgen::closure::Closure::once_into_js(Box::new(flush) as Box<dyn FnOnce()>);
    let frame_scheduled = web_sys::window()
        .and_then(|window| {
            window
                .request_animation_frame(frame.as_ref().unchecked_ref())
                .ok()
        })
        .is_some();
    if !frame_scheduled {
        let deltas = queue.borrow_mut().drain(..).collect::<Vec<_>>();
        let mut state = store.write();
        for (request_id, text) in deltas {
            state.append_stream_delta(&request_id, &text);
        }
        *scheduled.borrow_mut() = false;
    }
}

#[component]
pub fn AthenaPanel(props: AthenaPanelProps) -> Element {
    let mut athena_state = use_athena_store();
    let mut mounted = use_signal(|| false);
    let stream_delta_queue: Rc<RefCell<Vec<(String, String)>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let stream_delta_scheduled: Rc<RefCell<bool>> = use_hook(|| Rc::new(RefCell::new(false)));
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
        let stream_delta_queue_for_listener = stream_delta_queue.clone();
        let stream_delta_scheduled_for_listener = stream_delta_scheduled.clone();

        // athena:stream — request-scoped text and lifecycle events. Every
        // mutation is guarded by the active request ID in AthenaState.
        let mut stream_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:stream", move |payload: String| {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(&payload) else {
                return;
            };
            let request_id = event
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if request_id.is_empty() {
                return;
            }
            match event.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "delta" => {
                    if let Some(text) = event.get("text").and_then(|v| v.as_str()) {
                        stream_delta_queue_for_listener
                            .borrow_mut()
                            .push((request_id.to_string(), text.to_string()));
                        schedule_stream_flush(
                            stream_store,
                            stream_delta_queue_for_listener.clone(),
                            stream_delta_scheduled_for_listener.clone(),
                        );
                    }
                }
                "status" => {
                    if let Some(message) = event.get("message").and_then(|v| v.as_str()) {
                        if stream_store.read().accepts_stream_event(request_id) {
                            stream_store
                                .write()
                                .set_streaming_status(Some(message.to_string()));
                        }
                    }
                }
                "completed" => {
                    let final_text = event.get("text").and_then(|v| v.as_str());
                    stream_store.write().finish_stream(request_id, final_text);
                }
                "error" => {
                    let message = event
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Request failed");
                    let cancelled = event
                        .get("cancelled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    stream_store
                        .write()
                        .fail_stream(request_id, message.to_string(), cancelled);
                }
                _ => {}
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // athena:askUser — Show interactive user question modal.
        let mut ask_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:askUser", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let question_id = val
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let stream_request_id =
                    val.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
                if !stream_request_id.is_empty()
                    && !ask_store.read().accepts_stream_event(stream_request_id)
                {
                    return;
                }
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
                    .handle_ask_user(question_id, question, options);
            }
        }) {
            unlisteners_clone.borrow_mut().push(u);
        }

        // athena:planUpdate — Update plan display.
        let mut plan_store = store;
        if let Ok(u) = tauri_bridge::listen("athena:planUpdate", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let stream_request_id = val
                    .get("requestId")
                    .or_else(|| val.get("request_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if stream_request_id.is_empty()
                    || !plan_store.read().accepts_stream_event(stream_request_id)
                {
                    return;
                }
                let plan_id = val
                    .get("planId")
                    .or_else(|| val.get("plan_id"))
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
                let stream_request_id = val
                    .get("requestId")
                    .or_else(|| val.get("request_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if stream_request_id.is_empty()
                    || !eval_store.read().accepts_stream_event(stream_request_id)
                {
                    return;
                }
                let plan_id = val
                    .get("planId")
                    .or_else(|| val.get("plan_id"))
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
                    athena
                        .write()
                        .set_api_keyring_error(Some(format!("Keychain access failed: {:?}", e)));
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

    // Keep the log pinned to the newest content: any message/delta change
    // scrolls to the bottom — unless the user has scrolled up to read, in
    // which case the viewport is left alone until they return near bottom.
    use_effect(move || {
        let state = athena_state.read();
        let _ = state.messages.len();
        let _ = state.is_streaming;
        drop(state);
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(el) = doc.get_element_by_id("athena-message-log") {
                    let near_bottom =
                        el.scroll_height() - el.scroll_top() - el.client_height() < 80;
                    if near_bottom {
                        el.set_scroll_top(el.scroll_height());
                    }
                }
            }
        }
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
                ("var(--success)", "API key configured")
            }
        }
        Some(false) => (
            "var(--warning)",
            "API key not set. Configure it in Settings.",
        ),
        None => ("var(--textDim)", "Checking configuration..."),
    };

    let wrapper_style = match mode {
        AthenaPanelMode::Overlay => "position: absolute; bottom: 0; left: 0; right: 0; height: 35vh; display: flex; flex-direction: row; background: var(--bg); color: var(--text); border-top: 1px solid var(--border); z-index: 100;",
        AthenaPanelMode::Compact => "flex: 1; display: flex; flex-direction: row; min-width: 0; min-height: 0; background: var(--bg); color: var(--text); overflow: hidden;",
    };
    rsx! {
        div {
            class: "athena-panel",
            "data-athena-drop": "true",
            style: wrapper_style,

            // Main chat area
            div {
                style: "flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0;",

                // Header: session switcher, model, and status.
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                    // Session switcher dropdown
                    SessionSwitcher {}

                    span {
                        style: "flex: 1;",
                    }

                    span {
                        class: "badge",
                        style: "color: var(--accent);",
                        "{model_label}"
                    }

                    // Configuration status dot — reflects the backend's
                    // keyring-backed probe, not the in-memory defaults.
                    span {
                        title: "{status_title}",
                        "aria-label": "{status_title}",
                        role: "status",
                        style: "width: 8px; height: 8px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                    }
                }

                // Keyring failure warning — shows when the keychain is locked
                // but we have the confirmation flag, so the user knows why the
                // status dot may look odd.
                if let Some(ref err) = state.api_keyring_error {
                    div {
                        style: "display: flex; align-items: center; gap: 8px; padding: 6px 14px; background: rgba(235, 145, 19, 0.08); color: var(--warning); font-size: 12px;",
                        span { style: "flex-shrink: 0; display: inline-flex; align-items: center;",
                            IconWarning { size: Some(13), color: Some("var(--warning)".to_string()) }
                        }
                        span { "{err}" }
                    }
                }

                // Referenced pane bar. This is the durable acknowledgement for
                // a successful drag: it remains visible while the reference is
                // attached to the current Athena conversation, including when
                // Athena was opened by the drop itself.
                if !state.dropped_context.is_empty() {
                    div {
                        class: "athena-context-bar",
                        "data-athena-context-count": "{state.dropped_context.len()}",
                        role: "status",
                        "aria-live": "polite",
                        style: "background: transparent; padding: 5px 14px; font-size: 11px; color: var(--textMuted); display: flex; flex-wrap: wrap; gap: 4px; align-items: center;",
                        span { style: "font-weight: 600; color: var(--accent);", "Referenced:" }
                        for (i, item) in state.dropped_context.iter().enumerate() {
                            {
                                let display = match item {
                                    crate::stores::athena::DraggableItem::Agent { agent_type, label, .. } => {
                                        if agent_type == "shell" {
                                            format!("Shell: {}", label)
                                        } else {
                                            format!("Agent: {}", label)
                                        }
                                    },
                                    crate::stores::athena::DraggableItem::KanbanTask { title, .. } => format!("Task: {}", title),
                                    crate::stores::athena::DraggableItem::File { name, .. } => format!("File: {}", name),
                                };
                                rsx! {
                                    span {
                                        key: "context-{i}",
                                        "data-agent-pane-id": match item {
                                            crate::stores::athena::DraggableItem::Agent { pane_id, .. } => pane_id.clone(),
                                            _ => String::new(),
                                        },
                                        title: "Referenced by Athena",
                                        style: "padding: 2px 8px; border-radius: var(--radius-pill); background: var(--accentSubtle); color: var(--text); font-size: 10px;",
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

                // Messages. Keep the log on its own surface so the chat reads
                // as a deliberate workspace rather than transparent text over
                // the sidebar chrome.
                div {
                    class: "athena-message-log",
                    id: "athena-message-log",
                    style: "flex: 1; overflow-y: auto; padding: 14px 12px; display: flex; flex-direction: column; gap: 10px; background: var(--bg);",

                    if state.messages.is_empty() {
                        div {
                            class: "athena-empty-state",
                            style: "flex: 1; display: flex; align-items: center; justify-content: center;",
                        div {
                            style: "text-align: center; display: flex; flex-direction: column; align-items: center; gap: 8px; max-width: 270px;",
                            div { class: "athena-empty-mark", CoreMark { size: Some(44) } }
                            strong { "Start a conversation" }
                            span { "Ask for a plan, a refactor, or a second pair of eyes on the work in your workspace." }
                            div {
                                style: "display: flex; flex-direction: column; gap: 6px; width: 100%; margin-top: 8px;",
                                for prompt in [
                                    "Plan a refactor of this codebase",
                                    "Review my recent changes",
                                    "Explain how to add a new command",
                                ] {
                                    {
                                        let prompt = prompt.to_string();
                                        rsx! {
                                            button {
                                                class: "athena-suggestion-chip",
                                                onclick: move |_| {
                                                    let mut athena = athena_state;
                                                    // Don't fire a doomed request before an API key
                                                    // is configured — the composer banner covers it.
                                                    if matches!(athena.read().api_configured, Some(false)) {
                                                        return;
                                                    }
                                                    submit_message_text(&prompt, &mut athena);
                                                },
                                                "{prompt}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        }
                    } else {
                        // Only the message currently receiving deltas renders in
                        // word-blur streaming mode; settled history renders plain.
                        {
                            let streaming_msg_id = if state.is_streaming {
                                state.messages.back().map(|m| m.id.clone())
                            } else {
                                None
                            };
                            rsx! {
                                for msg in state.messages.iter() {
                                    {
                                        let streaming = state.is_streaming
                                            && streaming_msg_id.as_deref()
                                                == Some(msg.id.as_str());
                                        rsx! {
                                            AthenaChatMessage {
                                                key: "{msg.id}",
                                                message: msg.clone(),
                                                streaming,
                                            }
                                        }
                                    }
                                }
                            }
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
