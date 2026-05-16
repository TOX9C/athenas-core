use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use crate::stores::workspace::Space;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabProps {
    pub space: Space,
    pub is_active: bool,
    pub on_select: EventHandler<()>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn WorkspaceTab(props: WorkspaceTabProps) -> Element {
    let agent_status = use_agent_status_store();
    let bg = if props.is_active {
        "var(--bg)"
    } else {
        "transparent"
    };
    let border_bottom = if props.is_active {
        "2px solid var(--accent)"
    } else {
        "2px solid transparent"
    };
    let grid_label = format!("{:?}", props.space.grid);

    // Compute aggregate agent status for this space's panes.
    let mut any_error = false;
    let mut any_working = false;
    let mut idle_count = 0usize;
    let mut running_count = 0usize;
    for pane in props.space.panes.iter() {
        if let Some(status) = agent_status
            .read()
            .statuses
            .iter()
            .find(|(id, _)| id == &pane.id)
            .map(|(_, s)| &s.status)
        {
            match status {
                AgentRunStatus::Error => {
                    any_error = true;
                    running_count += 1;
                }
                AgentRunStatus::Working | AgentRunStatus::Thinking => {
                    any_working = true;
                    running_count += 1;
                }
                AgentRunStatus::WaitingForInput => {
                    running_count += 1;
                }
                AgentRunStatus::Idle | AgentRunStatus::Completed => {
                    idle_count += 1;
                }
                _ => {
                    idle_count += 1;
                }
            }
        } else {
            idle_count += 1;
        }
    }

    let status_dot_color = if any_error {
        "#e06c75"
    } else if any_working {
        "#e5c07b"
    } else {
        "#98c379"
    };

    rsx! {
        div {
            class: "workspace-tab",
            style: "display: flex; align-items: center; gap: 6px; padding: 4px 12px; border-radius: 6px 6px 0 0; cursor: pointer; background: {bg}; border-bottom: {border_bottom}; transition: background 0.15s; flex-shrink: 0;",
            onclick: move |_| props.on_select.call(()),

            // Status dot (green/orange/red based on aggregate agent status)
            div {
                style: "width: 8px; height: 8px; border-radius: 50%; background: {status_dot_color}; flex-shrink: 0;",
            }

            span {
                style: "font-size: 11px; color: var(--text); max-width: 100px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                "{props.space.name}"
            }

            // Agent count badges
            if idle_count > 0 {
                span {
                    style: "display: inline-flex; align-items: center; justify-content: center; min-width: 16px; height: 16px; border-radius: 8px; font-size: 9px; font-weight: 600; background: rgba(255,255,255,0.15); color: var(--text);",
                    "{idle_count}"
                }
            }

            if running_count > 0 {
                span {
                    style: "display: inline-flex; align-items: center; justify-content: center; min-width: 16px; height: 16px; border-radius: 8px; font-size: 9px; font-weight: 600; background: rgba(224,108,117,0.25); color: #e06c75;",
                    "{running_count}"
                }
            }

            // Grid label
            span {
                style: "font-size: 8px; padding: 1px 3px; border-radius: 3px; background: var(--bgTertiary); color: var(--textDim);",
                "{grid_label}"
            }

            // Close button
            button {
                style: "padding: 1px 3px; border-radius: 3px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 10px; opacity: 0.5;",
                onclick: move |e| {
                    e.stop_propagation();
                    props.on_close.call(());
                },
                "\u{00d7}"
            }
        }
    }
}
