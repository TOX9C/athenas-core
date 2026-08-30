use crate::components::shared::icon::{IconClose, IconMic};
use crate::stores::athena::{use_athena_store, AthenaMessage, AthenaState, MessageRole};
use crate::stores::ui::use_ui_store;
use crate::tauri_bridge;
use dioxus::prelude::*;
use wasm_bindgen::JsCast;

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

    // Include dropped context in the prompt. Clone the references before any
    // IPC so the signal guard is never held across an await.
    let referenced_context = athena_state.read().dropped_context.clone();
    let context_fragment = if referenced_context.is_empty() {
        String::new()
    } else {
        let mut parts: Vec<String> = vec!["\n[Referenced Context]".to_string()];
        for item in &referenced_context {
            match item {
                crate::stores::athena::DraggableItem::Agent {
                    pane_id,
                    agent_type,
                    label,
                } => {
                    let kind = if agent_type == "shell" {
                        "Shell"
                    } else {
                        "Agent"
                    };
                    parts.push(format!(
                        "- {kind} {}: {} (label: {})",
                        pane_id, agent_type, label
                    ));

                    // A reference must carry useful pane context, not only an
                    // identifier. Keep the snapshot bounded so a noisy PTY
                    // cannot consume the entire LLM request.
                    match tauri_bridge::output_buffer_get(pane_id, Some(240), None).await {
                        Ok(raw) => {
                            match serde_json::from_str::<Vec<tauri_bridge::OutputLine>>(&raw) {
                                Ok(lines) => {
                                    let output = lines
                                        .into_iter()
                                        .map(|line| line.text)
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    if !output.trim().is_empty() {
                                        let recent: String = output
                                            .chars()
                                            .rev()
                                            .take(12_000)
                                            .collect::<String>()
                                            .chars()
                                            .rev()
                                            .collect();
                                        parts.push(format!(
                                            "  Recent output:\n```text\n{recent}\n```"
                                        ));
                                    } else {
                                        parts.push(
                                            "  Recent output: (none captured yet)".to_string(),
                                        );
                                    }
                                }
                                Err(error) => {
                                    web_sys::console::warn_1(
                                        &format!(
                                            "[Athena] invalid output response for pane {}: {:?}",
                                            pane_id, error
                                        )
                                        .into(),
                                    );
                                    parts.push(
                                        "  Recent output: unavailable (invalid backend response)"
                                            .to_string(),
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            web_sys::console::warn_1(
                                &format!(
                                    "[Athena] failed to read output for pane {}: {:?}",
                                    pane_id, error
                                )
                                .into(),
                            );
                            parts.push(
                                "  Recent output: unavailable (backend read failed)".to_string(),
                            );
                        }
                    }
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

/// Grow the composer textarea up to its cap so the input feels like an
/// assistant composer rather than a fixed-height box. No resize handle.
fn autosize_composer() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(el) = document.get_element_by_id("athena-composer-input") else {
        return;
    };
    let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };
    let _ = html.style().set_property("height", "auto");
    let height = html.scroll_height();
    let _ = html
        .style()
        .set_property("height", &format!("{}px", height.min(132)));
}

/// Add the user message to the log and start the streaming request. Shared by
/// the composer, the empty-state quick prompts, and the inline retry action.
pub(crate) fn submit_message_text(text: &str, athena_state: &mut Signal<AthenaState>) {
    if text.trim().is_empty() {
        return;
    }

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

    let text_owned = text.to_string();
    let mut athena_state = *athena_state;
    spawn(async move {
        submit_message_async(text_owned, &mut athena_state).await;
    });
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
    input_text.set(String::new());
    autosize_composer();

    submit_message_text(text, athena_state);
}

/// Replay the last failed turn. Called from the inline Retry action on the
/// error message — keeping the composer free of transient action buttons.
pub(crate) fn retry_last_message(athena_state: &mut Signal<AthenaState>) {
    let text = athena_state
        .read()
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.content.clone());
    if let Some(text) = text {
        if athena_state.write().prepare_retry(&text) {
            submit_message_text(&text, athena_state);
        }
    }
}

/// Stop the voice capture, transcribe on-device, and append the transcript to
/// the composer. Shared by the mic button and the 30 s auto-stop timer.
fn stop_voice_recording(
    input_text: &Signal<String>,
    recording_voice: &Signal<bool>,
    voice_busy: &Signal<bool>,
    voice_error: &Signal<Option<String>>,
) {
    if *voice_busy.read() {
        return;
    }
    // Copy the handles out of the &-references (Signals are Copy) so the async
    // task owns them; Signal::set needs &mut, which a &Signal cannot give.
    let (mut input_text, mut recording_voice, mut voice_busy, mut voice_error) =
        (*input_text, *recording_voice, *voice_busy, *voice_error);
    voice_busy.set(true);
    spawn(async move {
        match tauri_bridge::voice_record_stop().await {
            Ok(text) => {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    let current = input_text.read().clone();
                    let combined = if current.trim().is_empty() {
                        trimmed
                    } else {
                        let sep = if current.ends_with(' ') || current.ends_with('\n') {
                            ""
                        } else {
                            " "
                        };
                        format!("{current}{sep}{trimmed}")
                    };
                    input_text.set(combined);
                    autosize_composer();
                }
            }
            Err(error) => {
                web_sys::console::warn_1(&format!("[voice] record stop failed: {error:?}").into());
                voice_error.set(Some(
                    "Couldn't transcribe the recording. Try again.".to_string(),
                ));
            }
        }
        recording_voice.set(false);
        voice_busy.set(false);
    });
}

#[component]
pub fn AthenaInput() -> Element {
    let mut athena_state = use_athena_store();
    let mut ui_state = use_ui_store();
    let mut input_text = use_signal(String::new);
    let mut input_history = use_signal(Vec::<String>::new);
    let mut history_idx = use_signal(|| None::<usize>);
    // Voice input state: recording toggle + busy/error lines. Recording is
    // on-device (mic → macOS speech recognition), so audio never leaves the Mac.
    let mut recording_voice = use_signal(|| false);
    let voice_busy = use_signal(|| false);
    let mut voice_error = use_signal(|| Option::<String>::None);
    let is_loading = athena_state.read().is_loading;
    let active_request_id = athena_state.read().active_request_id.clone();
    // Block sending until we've confirmed a key is set. This is what makes
    // the failure mode loud-and-clear ("set your key") instead of the old
    // behaviour where the request left, hit the env-var fallback, and came
    // back with a confusing orchestrator error.
    let api_configured = athena_state.read().api_configured;
    let is_blocked = matches!(api_configured, Some(false));
    // can_send is used for Enter key handling
    let can_send = !input_text.read().trim().is_empty() && !is_loading && !is_blocked;

    // Toggle voice capture: click to start, click again to stop & transcribe.
    // The 30 s auto-stop timer keeps a forgotten recording from running on
    // (the backend hard-caps the buffer at 60 s regardless).
    let mut toggle_voice = move || {
        if *voice_busy.read() || is_loading || is_blocked {
            return;
        }
        if *recording_voice.read() {
            stop_voice_recording(&input_text, &recording_voice, &voice_busy, &voice_error);
        } else {
            voice_error.set(None);
            spawn(async move {
                match tauri_bridge::voice_record_start().await {
                    Ok(()) => {
                        recording_voice.set(true);
                        let (it, rv, vb, ve) =
                            (input_text, recording_voice, voice_busy, voice_error);
                        spawn(async move {
                            gloo::timers::future::TimeoutFuture::new(30_000).await;
                            stop_voice_recording(&it, &rv, &vb, &ve);
                        });
                    }
                    Err(error) => {
                        web_sys::console::warn_1(
                            &format!("[voice] record start failed: {error:?}").into(),
                        );
                        voice_error.set(Some(
                            "Couldn't start the microphone. Check mic permissions.".to_string(),
                        ));
                    }
                }
            });
        }
    };

    // Cancel the in-flight generation.
    let cancel_stream = {
        let request_id = active_request_id.clone();
        move |_| {
            if let Some(request_id) = request_id.clone() {
                spawn(async move {
                    let _ = tauri_bridge::athena_cancel_stream(&request_id).await;
                });
            }
        }
    };

    // Hint line inside the composer toolbar — a single slot that swaps between
    // recording status, voice error, and the default send hint, so the composer
    // never shifts its height.
    let is_recording = *recording_voice.read();
    let voice_err = voice_error.read().clone();
    let has_voice_err = voice_err.is_some();
    let hint = if is_recording {
        "Listening... tap the mic to stop".to_string()
    } else if let Some(err) = voice_err.as_ref() {
        format!("Mic: {err}")
    } else if is_blocked {
        "Set an API key in Settings to start chatting".to_string()
    } else {
        "Enter to send · Shift+Enter newline".to_string()
    };

    rsx! {
        div {
            style: "padding: 10px 14px 12px; background: var(--bg); border-top: 1px solid var(--border); flex-shrink: 0;",

            // Banner shown when no API key is configured. Replaces the
            // "silently fails to send" experience with an actionable prompt.
            if is_blocked {
                div {
                    style: "display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 7px 10px; margin-bottom: 8px; border-radius: var(--radius-sm); background: rgba(235, 145, 19, 0.10);",
                    span {
                        style: "font-size: var(--text-xs); color: var(--warning);",
                        "No API key set: can't send messages yet."
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

            // Composer — a rounded field that grows with its content. The
            // toolbar row stays fixed so nothing jumps while streaming.
            div {
                class: "athena-composer",

                textarea {
                    id: "athena-composer-input",
                    rows: "1",
                    value: "{input_text}",
                    oninput: move |e| {
                        // Keep the controlled signal in sync with what the user
                        // types. Without this, `input_text` stays empty, every
                        // submit sees an empty string, and messages silently
                        // never send.
                        input_text.set(e.value());
                        // Typing breaks out of history navigation.
                        history_idx.set(None);
                        autosize_composer();
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        // Ignore Enter while blocked — there's nowhere to send.
                        if is_blocked || is_loading { return; }
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
                                autosize_composer();
                            }
                        } else if e.key() == Key::ArrowDown {
                            let hist = input_history.read();
                            if !hist.is_empty() {
                                let current = history_idx();
                                if let Some(i) = current {
                                    if i + 1 < hist.len() {
                                        history_idx.set(Some(i + 1));
                                        input_text.set(hist[i + 1].clone());
                                        autosize_composer();
                                    } else {
                                        history_idx.set(None);
                                        input_text.set(String::new());
                                        autosize_composer();
                                    }
                                }
                            }
                        }
                    },
                    placeholder: if is_blocked {
                        "Set an API key in Settings…"
                    } else {
                        "Type a message..."
                    },
                }

                // Toolbar — mic + hint on the left, send/stop on the right.
                div {
                    class: "athena-composer-toolbar",

                    button {
                        class: "athena-composer-mic",
                        style: if is_recording {
                            "background: var(--accent); color: var(--bg); box-shadow: inset 0 1px 0 rgba(255,255,255,0.14);".to_string()
                        } else {
                            "color: var(--textDim);".to_string()
                        },
                        title: if is_recording {
                            "Stop recording & transcribe".to_string()
                        } else {
                            "Speak to Athena (on-device)".to_string()
                        },
                        "aria-label": if is_recording {
                            "Stop voice recording".to_string()
                        } else {
                            "Start voice input".to_string()
                        },
                        disabled: is_loading || is_blocked || *voice_busy.read(),
                        onclick: move |_| toggle_voice(),
                        IconMic { size: Some(14), color: Some("currentColor".to_string()) }
                    }

                    span {
                        class: "athena-composer-hint",
                        style: if has_voice_err {
                            "color: var(--warning);"
                        } else {
                            "color: var(--textDim);"
                        },
                        if is_recording {
                            span { class: "athena-recording-dot" }
                        }
                        "{hint}"
                    }

                    // Keep a visible send action alongside the keyboard shortcut.
                    // This is especially important for discoverability and touch input.
                    if is_loading {
                        button {
                            class: "athena-composer-stop",
                            title: "Stop generating",
                            "aria-label": "Stop generating",
                            onclick: cancel_stream,
                            IconClose { size: Some(14), color: Some("currentColor".to_string()) }
                        }
                    } else {
                        button {
                            class: if can_send { "athena-composer-send is-ready" } else { "athena-composer-send" },
                            title: "Send message",
                            "aria-label": "Send message",
                            disabled: !can_send,
                            onclick: move |_| {
                                let text = input_text.read().clone();
                                submit_message(&text, &mut athena_state, &mut input_text, &mut input_history, &mut history_idx);
                            },
                            "↑"
                        }
                    }
                }
            }
        }
    }
}
