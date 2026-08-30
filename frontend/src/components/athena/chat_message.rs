use super::content_block::ContentBlockRenderer;
use crate::components::athena::athena_input::retry_last_message;
use crate::components::shared::icon::{IconCheck, IconCopy, IconRefresh};
use crate::stores::athena::{use_athena_store, AthenaMessage, MessageRole};
use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Props, Clone, PartialEq)]
pub struct ChatMessageProps {
    pub message: AthenaMessage,
    /// True when this message is the one currently receiving stream deltas,
    /// so its words resolve out of blur as they arrive.
    #[props(default = false)]
    pub streaming: bool,
}

/// Render the live assistant message word-by-word so each freshly arrived
/// word resolves out of a blur (stream-in). Earlier words keep their settled
/// spans (positional diffing preserves them), so the effect reads as a
/// continuous unfold rather than a full re-flash per delta.
fn streaming_content(content: &str) -> Element {
    let words: Vec<&str> = content.split(' ').collect();
    rsx! {
        for (i, word) in words.iter().enumerate() {
            span {
                key: "w-{i}",
                style: "display: inline; will-change: filter, opacity; animation: stream-in 420ms cubic-bezier(0.22,0.61,0.36,1) both;",
                "{word} "
            }
        }
        // Blinking caret while text is still arriving.
        span {
            aria_hidden: "true",
            style: "display: inline-block; width: 2px; height: 12px; margin-left: 2px; border-radius: 2px; background: var(--text); vertical-align: text-bottom; animation: blink-caret 1s step-end infinite;",
        }
    }
}

/// Copy plain text to the OS clipboard via the async Clipboard API.
async fn copy_to_clipboard(text: String) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(clipboard) = js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("clipboard"))
    else {
        return false;
    };
    let Ok(write_text) = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText")) else {
        return false;
    };
    let Ok(f) = write_text.dyn_into::<js_sys::Function>() else {
        return false;
    };
    f.call1(&clipboard, &JsValue::from_str(&text)).is_ok()
}

/// Copy affordance on assistant messages — swaps to a check for a beat after
/// a successful copy.
#[component]
fn CopyButton(content: String) -> Element {
    let copied = use_signal(|| false);
    rsx! {
        button {
            class: "athena-msg-copy",
            title: "Copy message",
            "aria-label": "Copy message",
            onclick: move |_| {
                let text = content.clone();
                let mut copied = copied;
                spawn(async move {
                    if copy_to_clipboard(text).await {
                        copied.set(true);
                        gloo::timers::future::TimeoutFuture::new(1_400).await;
                        copied.set(false);
                    }
                });
            },
            if copied() {
                IconCheck { size: Some(11), color: Some("var(--success)".to_string()) }
            } else {
                IconCopy { size: Some(11), color: Some("currentColor".to_string()) }
            }
        }
    }
}

#[component]
pub fn AthenaChatMessage(props: ChatMessageProps) -> Element {
    let msg = &props.message;
    let is_user = msg.role == MessageRole::User;
    let is_error = msg.is_error;
    // Hook calls must be unconditional — this drives the inline retry action.
    let athena_state = use_athena_store();

    // Hide empty assistant messages — when Athena is "thinking" but has
    // produced no content yet, don't render an empty card. The thinking
    // indicator below the message log handles that state.
    if !is_user && msg.content.is_empty() && msg.blocks.is_empty() && msg.images.is_empty() {
        return rsx! {};
    }

    let is_streaming = props.streaming;
    let has_content = !msg.content.is_empty();

    if is_user {
        rsx! {
            div {
                class: "athena-chat-row is-user",
                div {
                    class: "athena-user-message",
                    div {
                        class: "athena-user-text",
                        style: if is_error { "color: var(--error);" } else { "" },
                        "{msg.content}"
                    }
                }
            }
        }
    } else {
        let content_for_copy = msg.content.clone();
        rsx! {
            div {
                class: "athena-chat-row is-assistant",
                style: "align-items: flex-start; gap: 10px;",

                div {
                    style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",

                    // Content — plain text, no card.
                    if has_content {
                        div {
                            class: "athena-msg-text",
                            style: if is_error {
                                "color: var(--error);"
                            } else {
                                ""
                            },

                            if is_streaming && has_content {
                                {streaming_content(&msg.content)}
                            } else {
                                span { "{msg.content}" }
                            }

                            // Copy button inline with text.
                            if !is_error {
                                CopyButton { content: content_for_copy }
                            }
                        }
                    }

                    // Content blocks (plans, evaluations, ask-user, etc.)
                    for block in msg.blocks.iter() {
                        ContentBlockRenderer { key: "{block:?}", block: block.clone() }
                    }

                    // Image attachments.
                    for img in msg.images.iter() {
                        div {
                            key: "{img.id}",
                            style: "padding: 6px 10px; background: var(--bgTertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 10px; color: var(--textDim); width: fit-content;",
                            "IMG {img.name.as_deref().unwrap_or(\"image\")}"
                        }
                    }

                    // Inline retry — lives with the failure, not in the
                    // composer, so the input row never shifts.
                    if is_error {
                        div {
                            style: "display: flex; align-items: center; gap: 8px; padding-top: 2px;",
                            span {
                                style: "font-size: var(--text-xs); color: var(--error);",
                                "This request failed."
                            }
                            button {
                                class: "athena-msg-retry",
                                onclick: move |_| {
                                    let mut athena_state = athena_state;
                                    retry_last_message(&mut athena_state);
                                },
                                IconRefresh { size: Some(11), color: Some("currentColor".to_string()) }
                                "Retry"
                            }
                        }
                    }
                }
            }
        }
    }
}
