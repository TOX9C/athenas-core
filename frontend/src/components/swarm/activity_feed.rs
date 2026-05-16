use super::swarm_board::ActivityEntry;
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
                style: "padding: 8px 10px; border-bottom: 1px solid var(--border);",
                span {
                    style: "font-size: 11px; font-weight: 600; color: var(--text);",
                    "Activity"
                }
            }

            div {
                style: "flex: 1; overflow-y: auto; padding: 4px 0;",

                if props.activities.is_empty() {
                    div {
                        style: "padding: 16px; text-align: center; color: var(--textDim); font-size: 10px;",
                        "No activity yet"
                    }
                } else {
                    for entry in props.activities.iter() {
                        {
                            let role_color = match entry.role.as_str() {
                                "coordinator" => "#0ea5e9",
                                "builder" => "#22c55e",
                                "scout" => "#f59e0b",
                                "reviewer" => "#06b6d4",
                                _ => "var(--textDim)",
                            };
                            rsx! {
                                div {
                                    key: "{entry.id}",
                                    style: "padding: 6px 10px; border-bottom: 1px solid var(--border);",

                                    div {
                                        style: "display: flex; align-items: center; gap: 4px;",
                                        span {
                                            style: "font-size: 9px; font-weight: 600; color: {role_color};",
                                            "{entry.agent_name}"
                                        }
                                    }

                                    span {
                                        style: "font-size: 10px; color: var(--textMuted);",
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
