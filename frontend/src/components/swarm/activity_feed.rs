use super::swarm_board::ActivityEntry;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SwarmActivityFeedProps {
    pub activities: Vec<ActivityEntry>,
}

#[component]
pub fn SwarmActivityFeed(props: SwarmActivityFeedProps) -> Element {
    rsx! {
        div {
            class: "swarm-activity-feed",
            style: "display: flex; flex-direction: column; height: 100%;",

            div {
                style: "padding: 12px 14px; border-bottom: 1px solid var(--border);",
                span {
                    style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; letter-spacing: 0.04em; color: var(--accent);",
                    "Activity"
                }
            }

            div {
                style: "flex: 1; overflow-y: auto; overflow-x: hidden; padding: 2px 0; display: flex; flex-direction: column;",

                if props.activities.is_empty() {
                    EmptyState {
                        kind: EmptyArt::Swarm,
                        title: "No activity yet".to_string(),
                        hint: Some("Agent messages will appear here.".to_string()),
                    }
                } else {
                    for entry in props.activities.iter() {
                        {
                            let role_color = match entry.role.as_str() {
                                "coordinator" => "#0ea5e9",
                                "builder" => "#22c55e",
                                "scout" => "#f59e0b",
                                "reviewer" => "#06b6d4",
                                _ => "var(--accentTeal)",
                            };
                            rsx! {
                                div {
                                    key: "{entry.id}",
                                    style: "display: flex; gap: 8px; padding: 9px 14px; border-bottom: 1px solid var(--border);",

                                    span {
                                        style: "width: 6px; height: 6px; margin-top: 5px; border-radius: 50%; background: {role_color}; flex-shrink: 0;",
                                    }

                                    div {
                                        style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",
                                        span {
                                            style: "font-size: var(--text-2xs); font-weight: 600; color: {role_color};",
                                            "{entry.agent_name}"
                                        }
                                        span {
                                            style: "font-size: var(--text-xs); color: var(--textMuted); line-height: 1.4; word-break: break-word;",
                                            "{entry.action}"
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
