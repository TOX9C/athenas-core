use super::content_block::ContentBlockRenderer;
use crate::stores::athena::{AthenaMessage, MessageRole};
use dioxus::prelude::*;

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

#[component]
pub fn AthenaChatMessage(props: ChatMessageProps) -> Element {
    let msg = &props.message;
    let is_user = msg.role == MessageRole::User;
    let is_error = msg.is_error;

    // Hide empty assistant messages — when Athena is "thinking" but has
    // produced no content yet, don't render an empty card. The thinking
    // indicator below the message log handles that state.
    if !is_user && msg.content.is_empty() && msg.blocks.is_empty() && msg.images.is_empty() {
        return rsx! {};
    }

    let is_streaming = props.streaming;
    let has_content = !msg.content.is_empty();

    // Clean assistant interface: no avatars, no labels, no bubbles.
    // User messages get a subtle left accent bar; assistant messages are
    // plain readable text on the panel background.
    let (content_color, left_accent) = if is_error {
        ("var(--error)", "2px solid var(--error)")
    } else if is_user {
        ("var(--text)", "2px solid var(--accent)")
    } else {
        ("var(--text)", "2px solid transparent")
    };

    let content_bg = if is_error {
        "rgba(235, 145, 19, 0.06)"
    } else if is_user {
        "var(--bgSecondary)"
    } else {
        "transparent"
    };

    rsx! {
        div {
            class: "chat-message",
            style: "padding: 6px 0 6px 12px; border-left: {left_accent}; border-radius: 0 4px 4px 0; animation: fade-up 350ms cubic-bezier(0.22,0.61,0.36,1) both;",

            // Content — plain text, no bubble.
            if has_content {
                div {
                    style: "padding: 8px 12px; background: {content_bg}; color: {content_color}; border-radius: 0 4px 4px 0; font-size: 13px; line-height: 1.65; white-space: pre-wrap; word-break: break-word;",

                    if is_streaming && has_content {
                        {streaming_content(&msg.content)}
                    } else {
                        "{msg.content}"
                    }
                }
            }

            // Content blocks (plans, evaluations, ask-user, etc.)
            for block in msg.blocks.iter() {
                ContentBlockRenderer { key: "{block:?}", block: block.clone() }
            }

            // Image attachments — muted frost chips.
            for img in msg.images.iter() {
                div {
                    key: "{img.id}",
                    style: "margin-top: 4px; padding: 6px; background: var(--bgTertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 10px; color: var(--textDim);",
                    "IMG {img.name.as_deref().unwrap_or(\"image\")}"
                }
            }
        }
    }
}
