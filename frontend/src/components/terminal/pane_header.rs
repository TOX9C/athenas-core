use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use crate::stores::terminal::{use_terminal_store, PtyStatus};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PaneHeaderProps {
    pub pane_id: String,
    pub agent_type: String,
    pub is_fullscreen: bool,
    pub on_fullscreen: EventHandler<bool>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn PaneHeader(props: PaneHeaderProps) -> Element {
    let terminal_store = use_terminal_store();
    let agent_status = use_agent_status_store();

    let session_status = terminal_store
        .read()
        .sessions
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| s.status.clone());

    let run_status = agent_status
        .read()
        .statuses
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| s.status.clone());

    let (dot_color, status_text) = match session_status {
        Some(PtyStatus::Running) => ("#98c379", "running"),
        Some(PtyStatus::Ready) => ("#61afef", "ready"),
        Some(PtyStatus::Idle) => ("#5c6370", "idle"),
        Some(PtyStatus::Exited) => ("#5c6370", "exited"),
        Some(PtyStatus::Error) => ("#e06c75", "error"),
        None => match run_status {
            Some(AgentRunStatus::Thinking) => ("#61afef", "thinking"),
            Some(AgentRunStatus::Working) => ("#61afef", "working"),
            Some(AgentRunStatus::WaitingForInput) => ("#e5c07b", "waiting"),
            Some(AgentRunStatus::Error) => ("#e06c75", "error"),
            Some(AgentRunStatus::Completed) => ("#98c379", "done"),
            Some(AgentRunStatus::Cancelled) => ("#5c6370", "cancelled"),
            Some(AgentRunStatus::Disconnected) => ("#5c6370", "offline"),
            _ => ("#5c6370", "disconnected"),
        },
    };

    let agent_color = match props.agent_type.as_str() {
        "Claude" => "#7cb3f5",
        "Codex" => "#98c379",
        "OpenCode" => "#e5c07b",
        "Gemini" => "#56b6c2",
        "Shell" => "#5c6370",
        _ => "#5c6370",
    };

    rsx! {
        div {
            style: "height: 28px; background: var(--bgTertiary); border-bottom: 1px solid var(--border); display: flex; align-items: center; padding: 0 8px; gap: 6px; font-size: 11px; flex-shrink: 0;",

            // Status dot
            div {
                style: "width: 8px; height: 8px; border-radius: 50%; background: {dot_color}; flex-shrink: 0;",
            }

            // Agent type label
            span {
                style: "font-weight: 600; color: {agent_color}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                "{props.agent_type}"
            }

            // Status text
            span {
                style: "color: var(--textDim); font-size: 10px; white-space: nowrap;",
                "({status_text})"
            }

            // Pane ID
            span {
                style: "flex: 1; color: var(--textDim); font-size: 9px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: right; padding-right: 4px;",
                "{props.pane_id}"
            }

            // Fullscreen button
            button {
                style: "padding: 2px 6px; border-radius: 3px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px; line-height: 1;",
                onclick: move |_| {
                    let next = !props.is_fullscreen;
                    props.on_fullscreen.call(next);
                },
                if props.is_fullscreen { "\u{2921}" } else { "\u{26F6}" }
            }

            // Close button
            button {
                style: "padding: 2px 6px; border-radius: 3px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px; line-height: 1;",
                onclick: move |_| {
                    props.on_close.call(());
                },
                "\u{2715}"
            }
        }
    }
}
