use crate::components::shared::icon::IconClose;
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use crate::stores::workspace::Space;
use dioxus::prelude::*;
use std::rc::Rc;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabProps {
    pub space: Rc<Space>,
    pub is_active: bool,
    pub on_select: EventHandler<()>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn WorkspaceTab(props: WorkspaceTabProps) -> Element {
    let agent_status = use_agent_status_store();
    let bg = if props.is_active {
        "var(--bgTertiary)"
    } else {
        "transparent"
    };
    let text_color = if props.is_active {
        "var(--text)"
    } else {
        "var(--textDim)"
    };

    // Compute aggregate agent status for this space's panes.
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
                AgentRunStatus::Error |
                AgentRunStatus::Working |
                AgentRunStatus::Thinking |
                AgentRunStatus::WaitingForInput => {
                    running_count += 1;
                }
                _ => {
                    idle_count += 1;
                }
            }
        } else {
            idle_count += 1;
        }
    }

    rsx! {
        div {
            class: "workspace-tab",
            style: "display: flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: var(--radius-sm); cursor: pointer; background: {bg}; flex-shrink: 0; transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);",
            onclick: move |_| props.on_select.call(()),

            span {
                style: "font-size: var(--text-xs); color: {text_color}; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                "{props.space.name}"
            }

            // Agent count badges
            if idle_count > 0 {
                span {
                    class: "badge",
                    style: "min-width: 16px; height: 16px; padding: 0 4px; color: var(--textDim);",
                    "{idle_count}"
                }
            }

            if running_count > 0 {
                span {
                    class: "badge",
                    style: "min-width: 16px; height: 16px; padding: 0 4px; color: var(--warning); border-color: var(--warning);",
                    "{running_count}"
                }
            }

            // Close button
            button {
                class: "icon-btn",
                style: "width: 20px; height: 20px;",
                title: "Close workspace",
                "aria-label": "Close workspace",
                onclick: move |e| {
                    e.stop_propagation();
                    props.on_close.call(());
                },
                IconClose { size: Some(12), color: Some("currentColor".to_string()) }
            }
        }
    }
}
