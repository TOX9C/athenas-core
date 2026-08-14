use crate::components::shared::icon::{IconClose, IconRefresh, IconSend};
use crate::stores::athena::{use_athena_store, AthenaMessage, AthenaState, MessageRole};
use crate::stores::ui::use_ui_store;
use crate::tauri_bridge;
use dioxus::prelude::*;

/// Ensure the athena store has an active session ID. Creates one if not.
async fn ensure_session_id(athena_state: &mut Signal<AthenaState>) -> String {
    {
        if let Some(id) = &athena_state.read().session_id {
            return id.clone();
        }
    }

    let title = athena_state.read().session_title.clone();
    let create_result = tauri_bridge::session_create(Some(&title)).await;
    let session_json = match create_result {
        Ok(j) => j,
        Err(e) => {
            web_sys::console::warn_1(&format!("[ensure_session_id] failed: {:?}", e).into());
            return uuid::Uuid::new_v4().to_string();
        }
    };

    let session_id = match serde_json::from_str::<serde_json::Value>(&session_json) {
        Ok(val) => val
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        Err(_) => uuid::Uuid::new_v4().to_string(),
    };

    athena_state
        .write()
        .set_session_id(Some(session_id.clone()));
    session_id
}

/// Async body of the submit flow. The backend emits deltas on
/// `athena:stream`; this task only owns request startup/failure cleanup.
async fn submit_message_async(text: String, athena_state: &mut Signal<AthenaState>) {
    let session_id = ensure_session_id(athena_state).await;
    let request_id = uuid::Uuid::new_v4().to_string();

    // Include dropped context in the prompt.
    let context_fragment = {
        let athena_guard = athena_state.read();
        if athena_guard.dropped_context.is_empty() {
            String::new()
        } else {
            let mut parts: Vec<String> = vec!["\n[Pinned Context]".to_string()];
            for item in &athena_guard.dropped_context {
                match item {
                    crate::stores::athena::DraggableItem::Agent {
                        pane_id,
                        agent_type,
                        label,
                    } => {
                        parts.push(format!(
                            "- Agent {}: {} (label: {})",
                            pane_id, agent_type, label
                        ));
                    }
                    crate::stores::athena::DraggableItem::KanbanTask {
                        task_id,
                        title,
                        status,
                    } => {
                        parts.push(format!("- Kanban Task {}: {} ({})", task_id, title, status));
                    }
                    crate::stores::athena::DraggableItem::File { path, name } => {
                        parts.push(format!("- File: {} ({})", name, path));
                    }
                }
            }
            parts.push(String::new());
            parts.join("\n")
        }
    };
    let full_prompt = if context_fragment.is_empty() {
        text
    } else {
        format!("{}{}", text, context_fragment)
    };

    athena_state.write().begin_stream(request_id.clone());
    athena_state.write().add_message(AthenaMessage {
        id: format!("msg-{}", chrono::Utc::now().timestamp_millis()),
        role: MessageRole::Athena,
        content: String::new(),
        timestamp: chrono::Utc::now().timestamp(),
        is_error: false,
        images: Vec::new(),
        blocks: Vec::new(),
    });

    match tauri_bridge::athena_chat_stream(&full_prompt, &session_id, &request_id).await {
        Ok(_) => {
            // The authoritative completion event owns persistence. Saving
            // here would race the event listener and could overwrite the
            // completed assistant text with a partial placeholder.
        }
        Err(error) => {
            athena_state
                .write()
                .fail_stream(&request_id, format!("{:?}", error), false);
        }
    }
}

/// Submit the current input text to the Athena chat orchestrator.
fn submit_message(
    text: &str,
    athena_state: &mut Signal<AthenaState>,
    input_text: &mut Signal<String>,
    input_history: &mut Signal<Vec<String>>,
    history_idx: &mut Signal<Option<usize>>,
) {
    if text.trim().is_empty() {
        return;
    }

    // Push to input history
    let mut hist = input_history.write();
    hist.push(text.to_string());
    drop(hist);
    history_idx.set(None);

    // Add user message to local store
    let user_msg = AthenaMessage {
        id: format!("msg-{}", chrono::Utc::now().timestamp_millis()),
        role: MessageRole::User,
        content: text.to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        is_error: false,
        images: Vec::new(),
        blocks: Vec::new(),
    };
    athena_state.write().add_message(user_msg);
    input_text.set(String::new());

    let text_owned = text.to_string();
    let mut athena_state = *athena_state;
    spawn(async move {
        submit_message_async(text_owned, &mut athena_state).await;
    });
}

#[component]
pub fn AthenaInput() -> Element {
    let mut athena_state = use_athena_store();
    let mut ui_state = use_ui_store();
    let mut input_text = use_signal(String::new);
    let mut input_history = use_signal(Vec::<String>::new);
    let mut history_idx = use_signal(|| None::<usize>);
    let is_loading = athena_state.read().is_loading;
    let active_request_id = athena_state.read().active_request_id.clone();
    let retry_text = athena_state
        .read()
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.content.clone());
    // Block sending until we've confirmed a key is set. This is what makes
    // the failure mode loud-and-clear ("set your key") instead of the old
    // behaviour where the request left, hit the env-var fallback, and came
    // back with a confusing orchestrator error.
    let api_configured = athena_state.read().api_configured;
    let is_blocked = matches!(api_configured, Some(false));

    rsx! {
        div {
            style: "padding: 10px 14px; background: var(--bg); border-top: 1px solid var(--border); flex-shrink: 0;",

            // Banner shown when no API key is configured. Replaces the
            // "silently fails to send" experience with an actionable prompt.
            if is_blocked {
                div {
                    style: "display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 8px 10px; margin-bottom: 8px; border: 1px solid var(--warning); border-radius: var(--radius-sm); background: rgba(235, 145, 19, 0.10);",
                    span {
                        style: "font-size: var(--text-xs); color: var(--warning);",
                        "No API key set — Athena can't send messages yet."
                    }
                    button {
                        class: "btn-secondary btn-sm",
                        onclick: move |_| {
                            ui_state.write().show_settings_modal = true;
                        },
                        "Open Settings"
                    }
                }
            }

            // Input area
            div {
                style: "display: flex; gap: 8px; align-items: flex-end;",

                textarea {
                    class: "field",
                    style: "flex: 1; min-height: 40px; max-height: 120px; resize: vertical;",
                    value: "{input_text}",
                    oninput: move |e| {
                        // Keep the controlled signal in sync with what the user
                        // types. Without this, `input_text` stays empty, every
                        // submit sees an empty string, and messages silently
                        // never send.
                        input_text.set(e.value());
                        // Typing breaks out of history navigation.
                        history_idx.set(None);
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        // Ignore Enter while blocked — there's nowhere to send.
                        if is_blocked { return; }
                        if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) {
                            e.prevent_default();
                            let text = input_text.read().clone();
                            if !text.trim().is_empty() {
                                submit_message(&text, &mut athena_state, &mut input_text, &mut input_history, &mut history_idx);
                            }
                        } else if e.key() == Key::ArrowUp {
                            let hist = input_history.read();
                            if !hist.is_empty() {
                                let current = history_idx();
                                let new_idx = current.map_or(hist.len() - 1, |i| if i > 0 { i - 1 } else { 0 });
                                history_idx.set(Some(new_idx));
                                input_text.set(hist[new_idx].clone());
                            }
                        } else if e.key() == Key::ArrowDown {
                            let hist = input_history.read();
                            if !hist.is_empty() {
                                let current = history_idx();
                                if let Some(i) = current {
                                    if i + 1 < hist.len() {
                                        history_idx.set(Some(i + 1));
                                        input_text.set(hist[i + 1].clone());
                                    } else {
                                        history_idx.set(None);
                                        input_text.set(String::new());
                                    }
                                }
                            }
                        }
                    },
                    placeholder: if is_blocked {
                        "Set an API key in Settings to start chatting…".to_string()
                    } else {
                        "Ask Athena... (Shift+Enter for newline)".to_string()
                    },
                    disabled: is_loading || is_blocked,
                }

                if is_loading {
                    button {
                        class: "btn-secondary",
                        style: "padding: 0 14px; height: 40px; display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; color: var(--warning);",
                        title: "Stop generating",
                        onclick: {
                            let request_id = active_request_id.clone();
                            move |_| {
                                if let Some(request_id) = request_id.clone() {
                                    spawn(async move {
                                        let _ = tauri_bridge::athena_cancel_stream(&request_id).await;
                                    });
                                }
                            }
                        },
                        IconClose { size: Some(15), color: Some("currentColor".to_string()) }
                        "Stop"
                    }
                } else if let Some(retry_text) = retry_text.clone() {
                    if athena_state.read().error.is_some() {
                        button {
                            class: "btn-secondary",
                            style: "padding: 0 14px; height: 40px; display: inline-flex; align-items: center; gap: 6px; white-space: nowrap;",
                            title: "Retry last message",
                            onclick: move |_| {
                                let text = retry_text.clone();
                                if athena_state.write().prepare_retry(&text) {
                                    submit_message(&text, &mut athena_state, &mut input_text, &mut input_history, &mut history_idx);
                                }
                            },
                            IconRefresh { size: Some(15), color: Some("currentColor".to_string()) }
                            "Retry"
                        }
                    }
                }

                button {
                    class: "btn-primary",
                    // Single merged style — two `style:` attributes previously
                    // collided (last-writer-wins), so the entire inline style
                    // was replaced by just "opacity: 0.5;" / "" and the button
                    // lost its height/padding/layout on every render.
                    style: format!(
                        "padding: 0 16px; height: 40px; display: inline-flex; align-items: center; gap: 6px; white-space: nowrap;{}",
                        if is_loading || is_blocked { " opacity: 0.5;" } else { "" }
                    ),
                    title: "Send (Enter)",
                    onclick: move |_| {
                        if is_blocked { return; }
                        let text = input_text.read().clone();
                        submit_message(&text, &mut athena_state, &mut input_text, &mut input_history, &mut history_idx);
                    },
                    disabled: is_loading || is_blocked,
                    IconSend { size: Some(16), color: Some("currentColor".to_string()) }
                    "Send"
                }
            }
        }
    }
}
