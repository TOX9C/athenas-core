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
            class: "agent-card",
            style: "padding: 12px; border-radius: 8px; border: 1px solid var(--border); background: var(--bgSecondary); display: flex; flex-direction: column; gap: 8px;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 6px;",

                // Status dot (CSS circle, no emoji)
                div {
                    style: "width: 6px; height: 6px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                }

                span {
                    style: "font-size: 12px; font-weight: 600; color: var(--text); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{props.agent.id}"
                }

                SwarmRoleBadge { role: role_str }
            }

            // Status message
            if !props.agent.last_action.is_empty() {
                div {
                    style: "font-size: 10px; color: var(--textDim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{props.agent.last_action}"
                }
            }

            // Status label
            div {
                style: "font-size: 9px; color: {status_color}; text-transform: capitalize;",
                "{status_label}"
            }

            // Nudge button for stalled agents
            if matches!(props.agent.status, crate::stores::swarm::SwarmAgentStatus::Stalled | crate::stores::swarm::SwarmAgentStatus::Blocked) {
                button {
                    style: "align-self: flex-start; padding: 3px 8px; border-radius: 4px; border: 1px solid var(--warning); background: transparent; color: var(--warning); cursor: pointer; font-size: 9px;",
                    onclick: move |_| {
                        // TODO: nudge agent via Tauri IPC
                    },
                    "Nudge"
                }
            }
        }
    }
}
