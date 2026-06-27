use crate::components::shared::icon::IconAgents;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_status::{use_agent_status_store, AgentStatus, AgentRunStatus};
use crate::stores::athena::DraggableItem;
use dioxus::prelude::*;

#[component]
pub fn AgentPanel() -> Element {
    let agent_status = use_agent_status_store();
    let statuses = agent_status.read().statuses.clone();

    let display_data: Vec<(String, AgentStatus)> = statuses
        .iter()
        .map(|(id, s)| (id.clone(), s.clone()))
        .collect();

    rsx! {
        div {
            class: "sidebar-agent-panel",
            style: "display: flex; flex-direction: column; height: 100%;",

            div {
                style: "padding: 8px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 6px;",
                IconAgents { size: Some(15), color: Some("var(--accent)".to_string()) }
                span {
                    style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                    "Agents"
                }
            }

            if statuses.is_empty() {
                EmptyState {
                    kind: EmptyArt::Agents,
                    title: "No agents".to_string(),
                    hint: Some("Active agents will appear here.".to_string()),
                }
            } else {
                div {
                    style: "flex: 1; overflow-y: auto; overflow-x: hidden; padding: 4px 0;",

                    for (pane_id, status) in display_data {
                        {
                            let dot_color = match &status.status {
                                AgentRunStatus::Thinking => "var(--warning)",
                                AgentRunStatus::Working => "var(--warning)",
                                AgentRunStatus::WaitingForInput => "var(--accentTeal)",
                                AgentRunStatus::Completed => "var(--success)",
                                AgentRunStatus::Error => "var(--error)",
                                AgentRunStatus::Cancelled => "var(--textDim)",
                                AgentRunStatus::Disconnected => "var(--textDim)",
                                AgentRunStatus::Idle => "var(--success)",
                            };
                            let status_label = match &status.status {
                                AgentRunStatus::Idle => "idle",
                                AgentRunStatus::Thinking => "thinking",
                                AgentRunStatus::Working => "working",
                                AgentRunStatus::WaitingForInput => "waiting",
                                AgentRunStatus::Completed => "done",
                                AgentRunStatus::Error => "error",
                                AgentRunStatus::Cancelled => "cancelled",
                                AgentRunStatus::Disconnected => "offline",
                            };
                            let message_text = status.message.as_deref().unwrap_or("");
                            let progress_text = status.progress.as_ref().map_or(String::new(), |p| format!(" {}/{}", p.current, p.total));
                            rsx! {
                                div {
                                    key: "{pane_id}",
                                    style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; font-size: 10px; cursor: grab;",
                                    draggable: "true",
                                    ondragstart: move |_e| {
                                        let dt = _e.data_transfer();
                                        let item = DraggableItem::Agent {
                                            pane_id: pane_id.clone(),
                                            agent_type: "agent".to_string(),
                                            label: pane_id.clone(),
                                        };
                                        if let Ok(json) = serde_json::to_string(&item) {
                                            let _ = dt.set_data("text/plain", &json);
                                        }
                                    },

                                    div {
                                        style: "width: 6px; height: 6px; border-radius: 50%; background: {dot_color}; flex-shrink: 0;",
                                    }

                                    span {
                                        style: "color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1;",
                                        "{pane_id}"
                                    }

                                    span {
                                        style: "color: var(--textDim); font-size: var(--text-xs); flex-shrink: 0;",
                                        "{status_label}{progress_text}"
                                    }
                                }

                                if !message_text.is_empty() {
                                    div {
                                        style: "padding: 0 8px 4px 20px; font-size: 9px; color: var(--textDim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                        "{message_text}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
