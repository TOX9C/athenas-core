use super::role_badge::SwarmRoleBadge;
use crate::stores::swarm::SwarmAgent;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AgentCardProps {
    pub agent: SwarmAgent,
}

#[component]
pub fn AgentCard(props: AgentCardProps) -> Element {
    let status_color = match &props.agent.status {
        crate::stores::swarm::SwarmAgentStatus::Thinking
        | crate::stores::swarm::SwarmAgentStatus::Writing => "var(--accent)",
        crate::stores::swarm::SwarmAgentStatus::Done => "var(--success)",
        crate::stores::swarm::SwarmAgentStatus::Blocked
        | crate::stores::swarm::SwarmAgentStatus::Stalled => "var(--error)",
        crate::stores::swarm::SwarmAgentStatus::Waiting => "var(--warning)",
        _ => "var(--textDim)",
    };

    let is_active = matches!(
        props.agent.status,
        crate::stores::swarm::SwarmAgentStatus::Thinking
            | crate::stores::swarm::SwarmAgentStatus::Writing
    );

    let status_label = match &props.agent.status {
        crate::stores::swarm::SwarmAgentStatus::Thinking => "thinking",
        crate::stores::swarm::SwarmAgentStatus::Writing => "writing",
        crate::stores::swarm::SwarmAgentStatus::Done => "done",
        crate::stores::swarm::SwarmAgentStatus::Blocked => "blocked",
        crate::stores::swarm::SwarmAgentStatus::Stalled => "stalled",
        crate::stores::swarm::SwarmAgentStatus::Waiting => "waiting",
        _ => "idle",
    };

    let role_str = format!("{:?}", props.agent.role);

    // Compact flat star card — restyled for the constellation map. It
    // keeps the same props/signals/handlers/role-color hex contract; only
    // the visual layer changes (opaque fill + hairline border).
    rsx! {
        div {
            class: "agent-card",
            style: "display: flex; flex-direction: column; gap: 6px; padding: 10px 11px 11px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgSecondary);",

            // Header — status star + id + status pill
            div {
                style: "display: flex; align-items: center; gap: 7px;",

                // Status "star" — a CSS diamond (rotated square) tinted with
                // the role/status color, the focal glyph of each constellation
                // node. Active stars pulse (orbit-glow on the dot wrapper).
                div {
                    class: "",
                    style: "width: 9px; height: 9px; background: {status_color}; transform: rotate(45deg); flex-shrink: 0;",
                }

                span {
                    style: "font-size: var(--text-2xs); font-family: var(--fontFamily); color: var(--accent); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; letter-spacing: 0.02em;",
                    "{props.agent.id}"
                }

                // Status pill — flat chip with status-color edge
                div {
                    style: "font-size: var(--text-2xs); font-weight: 600; padding: 1px 7px; border-radius: var(--radius-pill); background: color-mix(in srgb, {status_color} 12%, transparent); color: {status_color}; border: 1px solid color-mix(in srgb, {status_color} 32%, transparent); text-transform: capitalize; white-space: nowrap;",
                    "{status_label}"
                }
            }

            // Status message — single muted line inside the card
            if !props.agent.last_action.is_empty() {
                div {
                    style: "font-size: var(--text-xs); color: var(--textMuted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; line-height: 1.4; padding-top: 2px; border-top: 1px solid var(--border);",
                    "{props.agent.last_action}"
                }
            }

            // Role badge
            div {
                style: "display: flex; align-items: center; gap: 4px; margin-top: 1px;",
                SwarmRoleBadge { role: role_str }
            }

            // Nudge button for stalled agents — flat btn-secondary
            if matches!(props.agent.status, crate::stores::swarm::SwarmAgentStatus::Stalled | crate::stores::swarm::SwarmAgentStatus::Blocked) {
                button {
                    class: "btn-secondary btn-sm",
                    style: "align-self: flex-start; margin-top: 2px;",
                    onclick: move |_| {
                        // TODO: nudge agent via Tauri IPC
                    },
                    "Nudge"
                }
            }
        }
    }
}
