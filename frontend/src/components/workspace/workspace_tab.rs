use crate::components::shared::context_menu::{ContextMenu, MenuItem};
use crate::components::shared::icon::IconClose;
use crate::stores::agent_status::use_agent_status_store;
use crate::stores::workspace::Space;
use crate::utils::space_counts::{count_space_agents, SpaceCounts};
use dioxus::prelude::*;
use std::rc::Rc;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabProps {
    pub space: Rc<Space>,
    pub is_active: bool,
    pub on_select: EventHandler<()>,
    pub on_close: EventHandler<()>,
}

/// Small status dot inside a count badge. `is_attention` pulses via the shared
/// `pulse-soft` keyframe so a finished / waiting agent is visible out of the
/// corner of the eye.
#[component]
fn StatusDot(class: String) -> Element {
    rsx! { span { class: "status-dot {class}" } }
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

    // Live agent counts for this space's panes: [working][total][attention].
    // `working` deliberately renders LEFT of `total` (the user's ask), with
    // `attention` as the rightmost, amber, pulsing badge.
    let counts: SpaceCounts = count_space_agents(&props.space.panes, &agent_status.read().statuses);

    rsx! {
        ContextMenu {
            items: vec![MenuItem::danger("Close workspace")],
            on_select: move |_| props.on_close.call(()),

            div {
                class: "workspace-tab",
                style: "display: flex; align-items: center; gap: 6px; height: var(--tb-tab-height); padding: 0 10px; border: none; border-radius: var(--radius-sm); cursor: pointer; background: transparent; flex-shrink: 0; transition: color var(--dur-fast) var(--ease);",
                onclick: move |_| props.on_select.call(()),

                span {
                    style: "font-size: var(--tb-tab-font); font-weight: {weight}; color: {text_color}; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; letter-spacing: 0.02em;",
                    "{props.space.name}"
                }

                // Working count — LEFT of the total. Gold dot + number.
                if counts.working > 0 {
                    span {
                        class: "badge",
                        style: "color: var(--accent);",
                        title: "{counts.working} agent(s) working",
                        "aria-label": "Agents working",
                        StatusDot { class: "is-working".to_string() }
                        "{counts.working}"
                    }
                }

                // Total agent count — the legacy "how many agents" badge.
                if counts.total > 0 {
                    span {
                        class: "badge",
                        style: "color: var(--textDim);",
                        title: "{counts.total} agent(s)",
                        "aria-label": "Agents",
                        "{counts.total}"
                    }
                }

                // Attention count — finished / waiting / errored. Amber + pulse.
                if counts.attention > 0 {
                    span {
                        class: "badge",
                        style: "color: var(--warning);",
                        title: "{counts.attention} agent(s) need attention",
                        "aria-label": "Agents needing attention",
                        StatusDot { class: "is-attention".to_string() }
                        "{counts.attention}"
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
}
