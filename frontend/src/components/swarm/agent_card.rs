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

    rsx! {
        div {
            class: "agent-card card",
            style: "display: flex; flex-direction: column; gap: 10px;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px;",

                // Status dot (CSS circle, no emoji)
                div {
                    class: if is_active { "pulse-soft" } else { "" },
                    style: "width: 8px; height: 8px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                }

                span {
                    style: "font-size: var(--text-2xs); font-family: var(--fontFamily); color: var(--textDim); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{props.agent.id}"
                }

                // Status pill
                div {
                    style: "font-size: var(--text-2xs); font-weight: 600; padding: 2px 8px; border-radius: var(--radius-pill); background: color-mix(in srgb, {status_color} 14%, transparent); color: {status_color}; text-transform: capitalize; white-space: nowrap;",
                    "{status_label}"
                }
            }

            // Status message
            if !props.agent.last_action.is_empty() {
                div {
                    style: "font-size: var(--text-xs); color: var(--textMuted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; line-height: 1.4;",
                    "{props.agent.last_action}"
                }
            }

            // Role badge
            div {
                style: "display: flex; align-items: center; gap: 4px;",
                SwarmRoleBadge { role: role_str }
            }

            // Nudge button for stalled agents
            if matches!(props.agent.status, crate::stores::swarm::SwarmAgentStatus::Stalled | crate::stores::swarm::SwarmAgentStatus::Blocked) {
                button {
                    class: "btn-secondary btn-sm",
                    style: "align-self: flex-start;",
                    onclick: move |_| {
                        // TODO: nudge agent via Tauri IPC
                    },
                    "Nudge"
                }
            }
        }
    }
}
