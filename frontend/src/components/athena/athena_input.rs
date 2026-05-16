use crate::stores::athena::{use_athena_store, AthenaMessage, AthenaState, MessageRole};
use crate::tauri_bridge;
use dioxus::prelude::*;

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
        timestamp: chrono::Utc::now().timestamp_millis(),
        is_error: false,
        images: Vec::new(),
        blocks: Vec::new(),
    };
    athena_state.write().add_message(user_msg);
    input_text.set(String::new());

    // Set loading state
    athena_state.write().set_loading(true);
    athena_state.write().set_streaming(true);
    athena_state
        .write()
        .set_streaming_status(Some("Sending to Athena...".to_string()));
    athena_state.write().clear_error();

    let message_text = text.to_string();
    let mut athena = *athena_state;

    // Spawn async task to call the orchestrator backend
    spawn(async move {
        match tauri_bridge::athena_chat(&message_text).await {
            Ok(response) => {
                let assistant_msg = AthenaMessage {
                    id: format!("msg-{}", chrono::Utc::now().timestamp_millis()),
                    role: MessageRole::Athena,
                    content: response,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    is_error: false,
                    images: Vec::new(),
                    blocks: Vec::new(),
                };
                athena.write().add_message(assistant_msg);
            }
            Err(e) => {
                let error_msg = AthenaMessage {
                    id: format!("msg-{}", chrono::Utc::now().timestamp_millis()),
                    role: MessageRole::Athena,
                    content: format!("Error: {:?}", e),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    is_error: true,
                    images: Vec::new(),
                    blocks: Vec::new(),
                };
                athena.write().add_message(error_msg);
                athena.write().set_error(Some(format!("{:?}", e)));
            }
        }
        athena.write().set_loading(false);
        athena.write().set_streaming(false);
        athena.write().set_streaming_status(None);
    });
}

#[component]
pub fn AthenaInput() -> Element {
    let mut athena_state = use_athena_store();
    let mut input_text = use_signal(String::new);
    let mut input_history = use_signal(Vec::<String>::new);
    let mut history_idx = use_signal(|| None::<usize>);
    let mut show_file_picker = use_signal(|| false);

    let is_loading = athena_state.read().is_loading;

    rsx! {
        div {
            style: "border-top: 1px solid var(--border); padding: 8px 12px; background: var(--bgSecondary); flex-shrink: 0;",

            // Image attachments placeholder
            div {
                style: "display: flex; align-items: center; gap: 4px; margin-bottom: 4px;",

                button {
                    style: "padding: 4px 8px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 10px; font-weight: 600;",
                    onclick: move |_| show_file_picker.set(!show_file_picker()),
                    "IMG"
                    " Attach"
                }

                if show_file_picker() {
                    span {
                        style: "font-size: 9px; color: var(--textDim);",
                        "TODO: file picker for image attachments"
                    }
                }
            }

            // Input area
            div {
                style: "display: flex; gap: 8px; align-items: flex-end;",

                textarea {
                    style: "flex: 1; min-height: 36px; max-height: 120px; padding: 8px 12px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; font-family: inherit; resize: vertical; outline: none;",
                    value: "{input_text}",
                    oninput: move |e| input_text.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
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
                    placeholder: "Ask Athena... (Shift+Enter for newline)",
                    disabled: is_loading,
                }

                button {
                    style: "padding: 8px 16px; border-radius: 8px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 12px; font-weight: 600; white-space: nowrap;",
                    onclick: move |_| {
                        let text = input_text.read().clone();
                        submit_message(&text, &mut athena_state, &mut input_text, &mut input_history, &mut history_idx);
                    },
                    disabled: is_loading,
                    "Send"
                }
            }
        }
    }
}
