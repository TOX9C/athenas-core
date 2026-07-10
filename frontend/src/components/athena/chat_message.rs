use super::content_block::ContentBlockRenderer;
use crate::components::shared::illustration::OwlMark;
use crate::stores::athena::{AthenaMessage, MessageRole};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ChatMessageProps {
    pub message: AthenaMessage,
}

#[component]
pub fn AthenaChatMessage(props: ChatMessageProps) -> Element {
    let msg = &props.message;
    let is_user = msg.role == MessageRole::User;
    let is_error = msg.is_error;

    let align = if is_user { "flex-end" } else { "flex-start" };

    let content_color = if is_error {
        "var(--error)"
    } else {
        "var(--text)"
    };

    let body_border = if is_error {
        "1px solid var(--error)"
    } else {
        "none"
    };

    // Flat-quiet: messages read as text in a column, not as a stack of frosted
    // boxes. Errors keep a solid warning-tinted fill so the failure reads
    // clearly; everything else is transparent.
    let (plaque_bg, plaque_shadow) = if is_error {
        ("rgba(235, 145, 19, 0.10)", "none")
    } else {
        ("transparent", "none")
    };

    // Avatar: flat disc; Athena side carries the lit-sweep hover affordance.
    let (avatar_class, avatar_glow) = if is_user {
        ("", "none")
    } else {
        ("lit-sweep", "none")
    };

    let time_str = {
        // Simple timestamp formatting
        let secs = msg.timestamp;
        let hours = ((secs / 3600) % 24) as u8;
        let mins = ((secs / 60) % 60) as u8;
        format!("{:02}:{:02}", hours, mins)
    };

    rsx! {
        div {
            class: "chat-message",
            style: "display: flex; align-items: flex-start; gap: 10px; align-self: {align}; max-width: 90%; padding: 8px 0;",

            // Avatar — frost-light plaque with lit-sweep on Athena side.
            div {
                class: "{avatar_class}",
                style: "width: 28px; height: 28px; border-radius: 50%; background: var(--bgTertiary); border: 1px solid var(--border); box-shadow: {avatar_glow}; display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                if is_user {
                    span {
                        style: "font-size: var(--text-2xs); font-weight: 700; color: var(--accent); letter-spacing: 0.04em;",
                        "U"
                    }
                } else {
                    OwlMark { size: Some(18) }
                }
            }

            // Message body
            div {
                style: "flex: 1; min-width: 0;",

                // Header
                div {
                    style: "display: flex; align-items: center; gap: 8px; margin-bottom: 6px;",
                    span {
                        style: "font-size: var(--text-2xs); font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--textMuted);",
                        if is_user { "You" } else { "Athena" }
                    }
                    span {
                        style: "font-size: var(--text-2xs); color: var(--textDim);",
                        "{time_str}"
                    }
                }

                // Content — frost-light plaque body.
                div {
                    style: "padding: 12px 16px; background: {plaque_bg}; color: {content_color}; border: 1px solid {body_border}; border-radius: var(--radius-md); box-shadow: {plaque_shadow}; font-size: 13px; line-height: 1.6; white-space: pre-wrap; word-break: break-word;",

                    "{msg.content}"
                }

                // Content blocks
                for block in msg.blocks.iter() {
                    ContentBlockRenderer { key: "{block:?}", block: block.clone() }
                }

                // Image attachments — muted frost chips.
                for img in msg.images.iter() {
                    div {
                        key: "{img.id}",
                        style: "margin-top: 8px; padding: 6px; background: var(--bgTertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 10px; color: var(--textDim);",
                        "IMG {img.name.as_deref().unwrap_or(\"image\")}"
                    }
                }
            }
        }
    }
}
