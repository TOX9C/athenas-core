use super::content_block::ContentBlockRenderer;
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

    let (avatar_text, avatar_bg, avatar_color, bg, align) = if is_user {
        (
            "U",
            "var(--bgTertiary)",
            "var(--text)",
            "var(--bgTertiary)",
            "flex-end",
        )
    } else {
        (
            "A",
            "#38bdf822",
            "#38bdf8",
            "var(--bgSecondary)",
            "flex-start",
        )
    };

    let border_color = if is_error {
        "var(--error)"
    } else {
        "var(--border)"
    };
    let content_color = if is_error {
        "var(--error)"
    } else {
        "var(--text)"
    };

    let time_str = {
        // Simple timestamp formatting
        let secs = msg.timestamp / 1000;
        let hours = ((secs / 3600) % 24) as u8;
        let mins = ((secs / 60) % 60) as u8;
        format!("{:02}:{:02}", hours, mins)
    };

    rsx! {
        div {
            class: "chat-message",
            style: "display: flex; align-items: flex-start; gap: 8px; align-self: {align}; max-width: 85%;",

            // Avatar
            div {
                style: "width: 28px; height: 28px; border-radius: 8px; background: {avatar_bg}; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 700; color: {avatar_color}; flex-shrink: 0;",
                "{avatar_text}"
            }

            // Message body
            div {
                style: "flex: 1; min-width: 0;",

                // Header
                div {
                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 4px;",
                    span {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        if is_user { "You" } else { "Athena" }
                    }
                    span {
                        style: "font-size: 9px; color: var(--textDim);",
                        "{time_str}"
                    }
                }

                // Content
                div {
                    style: "padding: 10px 14px; border-radius: 8px; border: 1px solid {border_color}; background: {bg}; color: {content_color}; font-size: 12px; line-height: 1.5; white-space: pre-wrap; word-break: break-word;",

                    "{msg.content}"
                }

                // Content blocks
                for block in msg.blocks.iter() {
                    ContentBlockRenderer { key: "{block:?}" , block: block.clone() }
                }

                // Image attachments
                for img in msg.images.iter() {
                    div {
                        key: "{img.id}",
                        style: "margin-top: 6px; padding: 6px; border-radius: 6px; background: var(--bgTertiary); font-size: 10px; color: var(--textDim);",
                        "IMG {img.name.as_deref().unwrap_or(\"image\")}"
                    }
                }
            }
        }
    }
}
