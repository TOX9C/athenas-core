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
    // Selected space is marked by gold + bold text only — no bottom-edge
    // hairline underline. Matches the app vocabulary (.icon-btn.is-active,
    // .segmented-item.is-active both signal active via color, not a rule).
    let text_color = if props.is_active {
        "var(--accent)"
    } else {
        "var(--textMuted)"
    };
    let weight = if props.is_active { "600" } else { "400" };

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
            if matches!(
                status,
                AgentRunStatus::Error
                    | AgentRunStatus::Working
                    | AgentRunStatus::Thinking
                    | AgentRunStatus::WaitingForInput
            ) {
                running_count += 1;
            } else {
                idle_count += 1;
            }
        } else {
            idle_count += 1;
        }
    }

    rsx! {
        div {
            class: "workspace-tab",
            style: "display: flex; align-items: center; gap: 6px; height: var(--tb-tab-height); padding: 0 10px; border: none; border-radius: var(--radius-sm); cursor: pointer; background: transparent; flex-shrink: 0; transition: color var(--dur-fast) var(--ease);",
            onclick: move |_| props.on_select.call(()),

            span {
                style: "font-size: var(--tb-tab-font); font-weight: {weight}; color: {text_color}; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; letter-spacing: 0.02em;",
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
