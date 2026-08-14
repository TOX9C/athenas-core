use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use dioxus::prelude::*;

#[component]
pub fn AgentStatusList() -> Element {
    let agent_status = use_agent_status_store();

    let statuses = agent_status.read().statuses.clone();

    rsx! {
        div {
            class: "agent-status-list pane-astrolabe-mark",
            // The sidebar is already the section surface. Keep this list flush
            // with it instead of drawing a rounded panel inside another panel.
            style: "display: flex; flex-direction: column; height: 100%; flex: 1; overflow-y: auto; overflow-x: hidden; background: transparent; border: none; border-radius: 0;",

            if statuses.is_empty() {
                EmptyState {
                    kind: EmptyArt::Agents,
                    title: "No agents".to_string(),
                    hint: Some("Running agents will appear here.".to_string()),
                }
            } else {
                for (id, status) in statuses.iter() {
                    {
                        let (status_color, status_label) = match status.status {
                            AgentRunStatus::Thinking | AgentRunStatus::Working => ("var(--accent)", "Working"),
                            AgentRunStatus::Completed => ("var(--success)", "Done"),
                            AgentRunStatus::Error => ("var(--error)", "Error"),
                            AgentRunStatus::WaitingForInput => ("var(--warning)", "Waiting"),
                            AgentRunStatus::Cancelled => ("var(--textDim)", "Cancelled"),
                            AgentRunStatus::Disconnected => ("var(--textDim)", "Offline"),
                            AgentRunStatus::Idle => ("var(--textDim)", "Idle"),
                        };
                        let entry_id = id.clone();
                        let entry_name = id.clone();
                        let msg = status.message.clone().unwrap_or_default();
                        let msg_preview: String = msg.chars().take(60).collect();
                        rsx! {
                            div {
                                key: "{entry_id}",
                                class: "lit-sweep",
                                style: "display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); transition: box-shadow var(--dur-fast) var(--ease);",

                                div {
                                    style: "flex: 1; min-width: 0;",
                                    div {
                                        style: "display: flex; align-items: center; gap: 6px;",
                                        span {
                                            style: "font-size: var(--text-sm); font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{entry_name}"
                                        }
                                        span {
                                            class: "status-label",
                                            style: "color: {status_color};",
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
