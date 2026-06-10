use crate::components::shared::icon::IconHelmet;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use dioxus::prelude::*;

#[component]
pub fn AgentStatusList() -> Element {
    let agent_status = use_agent_status_store();

    let statuses = agent_status.read().statuses.clone();

    rsx! {
        div {
            class: "agent-status-list",
            style: "display: flex; flex-direction: column; height: 100%; flex: 1; overflow-y: auto;",

            if statuses.is_empty() {
                EmptyState {
                    kind: EmptyArt::Agents,
                    title: "No agents".to_string(),
                    hint: Some("Running agents will appear here.".to_string()),
                }
            } else {
                for (id, status) in statuses.iter() {
                    {
                        let (status_color, status_label, is_working) = match status.status {
                            AgentRunStatus::Thinking | AgentRunStatus::Working => ("var(--accent)", "Working", true),
                            AgentRunStatus::Completed => ("var(--success)", "Done", false),
                            AgentRunStatus::Error => ("var(--error)", "Error", false),
                            AgentRunStatus::WaitingForInput => ("var(--warning)", "Waiting", false),
                            AgentRunStatus::Cancelled => ("var(--textDim)", "Cancelled", false),
                            AgentRunStatus::Disconnected => ("var(--textDim)", "Offline", false),
                            AgentRunStatus::Idle => ("var(--textDim)", "Idle", false),
                        };
                        let entry_id = id.clone();
                        let entry_name = id.clone();
                        let msg = status.message.clone().unwrap_or_default();
                        let msg_preview: String = msg.chars().take(60).collect();
                        let dot_class = if is_working { "pulse-soft" } else { "" };
                        rsx! {
                            div {
                                key: "{entry_id}",
                                style: "display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border);",

                                div {
                                    class: "{dot_class}",
                                    style: "width: 8px; height: 8px; border-radius: var(--radius-pill); background: {status_color}; flex-shrink: 0;",
                                }

                                div {
                                    style: "flex: 1; min-width: 0;",
                                    div {
                                        style: "display: flex; align-items: center; gap: 6px;",
                                        span {
                                            style: "font-size: var(--text-sm); font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{entry_name}"
                                        }
                                        span {
                                            style: "font-size: var(--text-2xs); padding: 1px 6px; border-radius: var(--radius-pill); background: color-mix(in srgb, {status_color} 16%, transparent); color: {status_color};",
                                            "{status_label}"
                                        }
                                    }
                                    if !msg_preview.is_empty() {
                                        span {
                                            style: "font-size: var(--text-2xs); color: var(--textDim); display: block; margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{msg_preview}"
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
}
