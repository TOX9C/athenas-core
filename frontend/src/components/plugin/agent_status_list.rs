use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use dioxus::prelude::*;

#[component]
pub fn AgentStatusList() -> Element {
    let agent_status = use_agent_status_store();

    let statuses = agent_status.read().statuses.clone();

    rsx! {
        div {
            class: "agent-status-list",
            style: "display: flex; flex-direction: column; height: 100%;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; border-bottom: 1px solid var(--border);",
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    span {
                        style: "font-size: 9px; font-weight: 700; color: var(--accent); letter-spacing: 0.06em;",
                        "AG"
                    }
                    span {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        "Agents"
                    }
                }
            }

            // Agent list
            div {
                style: "flex: 1; overflow-y: auto;",

                if statuses.is_empty() {
                    div {
                        style: "display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 32px; gap: 8px; color: var(--textDim);",
                        span {
                            style: "font-size: 11px; font-weight: 700; opacity: 0.25; letter-spacing: 0.06em;",
                            "AG"
                        }
                        span { style: "font-size: 10px;", "No agents active" }
                        span { style: "font-size: 9px;", "Launch a terminal to see agent status" }
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
                                    style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border);",

                                    div {
                                        style: "width: 8px; height: 8px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                                    }

                                    div {
                                        style: "flex: 1; min-width: 0;",
                                        div {
                                            style: "display: flex; align-items: center; gap: 4px;",
                                            span {
                                                style: "font-size: 11px; font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                                "{entry_name}"
                                            }
                                            span {
                                                style: "font-size: 9px; padding: 1px 4px; border-radius: 3px; background: {status_color}22; color: {status_color};",
                                                "{status_label}"
                                            }
                                        }
                                        if !msg_preview.is_empty() {
                                            span {
                                                style: "font-size: 9px; color: var(--textDim); display: block; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
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
}
