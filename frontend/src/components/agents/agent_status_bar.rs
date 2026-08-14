use crate::components::shared::icon::IconPulse;
use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::use_agent_status_store;
use crate::utils::agent_display::get_agent_color_str;
use dioxus::prelude::*;

#[path = "agent_status_bar_model.rs"]
mod agent_status_bar_model;

use agent_status_bar_model::{status_label, time_ago, to_pane_status};
pub use agent_status_bar_model::{AgentPaneStatus, ProgressInfo};

#[derive(Props, Clone, PartialEq)]
pub struct AgentStatusBarProps {
    pub pane_id: String,
}

#[component]
pub fn AgentStatusBar(props: AgentStatusBarProps) -> Element {
    let agent_status = use_agent_status_store();
    let agent_output = use_agent_output_store();

    let current_status: AgentPaneStatus = agent_status
        .read()
        .statuses
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| to_pane_status(s))
        .unwrap_or_default();

    let line_count: usize = agent_output
        .read()
        .buffers
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, lines)| lines.len())
        .unwrap_or(0);

    let (label, word, color) = status_label(&current_status.status);
    let agent_color = get_agent_color_str(&current_status.agent_type);
    let display_id: String = props.pane_id.chars().take(10).collect();
    let msg_preview: String = current_status.message.chars().take(40).collect();
    let ago = time_ago(current_status.last_updated_at);

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-top: 1px solid var(--border); flex-shrink: 0; overflow-x: hidden;",

            // Agent helmet glyph
            span {
                style: "display: inline-flex; align-items: center; color: {agent_color}; flex-shrink: 0;",
                IconPulse { size: Some(14), color: Some("currentColor".to_string()) }
            }

            // Status label. The word carries the state without a capsule or
            // accent marker competing with the agent identity glyph.
            span {
                class: "status-label",
                style: "color: {color}; flex-shrink: 0;",
                span {
                    style: "font-family: var(--font-display);",
                    title: "{label}",
                    "{word}"
                }
            }

            // Pane id
            span {
                style: "font-size: var(--text-2xs); font-family: var(--fontFamily); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--textMuted); flex-shrink: 0;",
                "{display_id}"
            }

            // Message preview
            if !current_status.message.is_empty() {
                span {
                    style: "font-size: var(--text-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; color: var(--textDim);",
                    "{msg_preview}"
                }
            }

            // Line count
            if line_count > 0 {
                span {
                    style: "font-size: var(--text-2xs); flex-shrink: 0; color: var(--textDim);",
                    "{line_count} lines"
                }
            }

            // Time ago
            span {
                style: "font-size: var(--text-2xs); flex-shrink: 0; color: var(--textDim);",
                "{ago}"
            }
        }
    }
}
