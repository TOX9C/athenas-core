use super::plugin_dashboard::PluginData;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PluginCardProps {
    pub plugin: PluginData,
}

#[component]
pub fn PluginCard(props: PluginCardProps) -> Element {
    let (status_color, status_label) = match props.plugin.status.as_str() {
        "active" => ("var(--success)", "Active".to_string()),
        "error" => ("var(--error)", "Error".to_string()),
        "installing" | "updating" => ("var(--accent)", props.plugin.status.clone()),
        _ => ("var(--textDim)", "Inactive".to_string()),
    };

    let border_color = if props.plugin.status == "error" {
        "var(--error)"
    } else {
        "var(--border)"
    };
    let opacity = if props.plugin.enabled { "1" } else { "0.6" };
    let toggle_bg = if props.plugin.enabled {
        "var(--success)"
    } else {
        "var(--bgTertiary)"
    };
    let toggle_color = if props.plugin.enabled {
        "#fff"
    } else {
        "var(--textDim)"
    };
    let toggle_label = if props.plugin.enabled { "ON" } else { "OFF" };
    let _status_bg = format!("{}22", status_color);
    let initial: String = props.plugin.name.chars().next().unwrap_or('?').to_string();

    rsx! {
        div {
            class: "plugin-card",
            style: "padding: 12px; border-radius: 8px; border: 1px solid {border_color}; background: var(--bgSecondary); opacity: {opacity}; display: flex; flex-direction: column; gap: 8px;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",

                div {
                    style: "display: flex; align-items: center; gap: 6px;",

                    div {
                        style: "width: 24px; height: 24px; border-radius: 4px; background: var(--bgTertiary); display: flex; align-items: center; justify-content: center; font-size: 10px; font-weight: 700; color: var(--textMuted);",
                        "{initial}"
                    }

                    div {
                        span {
                            style: "font-size: 11px; font-weight: 600; color: var(--text); display: block;",
                            "{props.plugin.name}"
                        }
                        span {
                            style: "font-size: 9px; color: var(--textDim);",
                            "v{props.plugin.version} \u{00b7} {props.plugin.author}"
                        }
                    }
                }

                // Toggle
                button {
                    style: "padding: 4px 8px; border-radius: 4px; border: none; background: {toggle_bg}; color: {toggle_color}; cursor: pointer; font-size: 10px;",
                    onclick: move |_| {
                        // TODO: toggle plugin via Tauri IPC
                    },
                    "{toggle_label}"
                }
            }

            // Description
            p {
                style: "font-size: 10px; color: var(--textMuted); margin: 0; line-height: 1.4;",
                "{props.plugin.description}"
            }

            // Status + capabilities
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",

                div {
                    style: "display: flex; align-items: center; gap: 4px;",
                    // Status dot (CSS circle)
                    div {
                        style: "width: 5px; height: 5px; border-radius: 50%; background: {status_color};",
                    }
                    span {
                        style: "font-size: 9px; color: {status_color};",
                        "{status_label}"
                    }
                    if props.plugin.agent_count > 0 {
                        span {
                            style: "font-size: 9px; color: var(--textDim);",
                            "\u{00b7} {props.plugin.agent_count} agents"
                        }
                    }
                }

                if !props.plugin.capabilities.is_empty() {
                    div {
                        style: "display: flex; gap: 3px;",
                        for cap in props.plugin.capabilities.iter().take(3) {
                            span {
                                key: "{cap}",
                                style: "font-size: 8px; padding: 1px 3px; border-radius: 2px; background: var(--bgTertiary); color: var(--textDim);",
                                "{cap}"
                            }
                        }
                    }
                }
            }

            // Error display
            if let Some(err) = &props.plugin.error {
                div {
                    style: "font-size: 9px; padding: 4px 8px; border-radius: 4px; background: var(--error); color: #fff; opacity: 0.9;",
                    "{err}"
                }
            }
        }
    }
}
