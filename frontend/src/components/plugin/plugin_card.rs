use super::plugin_dashboard::PluginData;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PluginCardProps {
    pub plugin: PluginData,
}

#[component]
pub fn PluginCard(props: PluginCardProps) -> Element {
    let is_running = props.plugin.status == "active";
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
    let toggle_label = if props.plugin.enabled { "ON" } else { "OFF" };
    let toggle_class = if props.plugin.enabled {
        "btn-primary btn-sm"
    } else {
        "btn-secondary btn-sm"
    };
    let initial: String = props
        .plugin
        .name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let dot_class = if is_running { "pulse-soft" } else { "" };

    rsx! {
        div {
            class: "card",
            style: "border-color: {border_color}; opacity: {opacity}; display: flex; flex-direction: column; gap: 10px;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",

                div {
                    style: "display: flex; align-items: center; gap: 8px;",

                    div {
                        style: "width: 28px; height: 28px; border-radius: var(--radius-md); background: var(--accentSubtle); display: flex; align-items: center; justify-content: center; font-family: var(--font-display); font-size: 14px; font-weight: 600; color: var(--accent); flex-shrink: 0;",
                        "{initial}"
                    }

                    div {
                        span {
                            style: "font-size: var(--text-sm); font-weight: 600; color: var(--text); display: block;",
                            "{props.plugin.name}"
                        }
                        span {
                            style: "font-size: var(--text-2xs); color: var(--textDim);",
                            "v{props.plugin.version} \u{00b7} {props.plugin.author}"
                        }
                    }
                }

                // Toggle
                button {
                    class: "{toggle_class}",
                    onclick: move |_| {
                        // TODO: toggle plugin via Tauri IPC
                    },
                    "{toggle_label}"
                }
            }

            // Description
            p {
                style: "font-size: var(--text-xs); color: var(--textMuted); margin: 0; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                "{props.plugin.description}"
            }

            // Status + capabilities
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",

                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    // Status dot
                    div {
                        class: "{dot_class}",
                        style: "width: 6px; height: 6px; border-radius: var(--radius-pill); background: {status_color}; flex-shrink: 0;",
                    }
                    span {
                        style: "font-size: var(--text-2xs); color: {status_color};",
                        "{status_label}"
                    }
                    if props.plugin.agent_count > 0 {
                        span {
                            style: "font-size: var(--text-2xs); color: var(--textDim);",
                            "\u{00b7} {props.plugin.agent_count} agents"
                        }
                    }
                }

                if !props.plugin.capabilities.is_empty() {
                    div {
                        style: "display: flex; gap: 4px;",
                        for cap in props.plugin.capabilities.iter().take(3) {
                            span {
                                key: "{cap}",
                                class: "pill",
                                style: "font-size: var(--text-2xs);",
                                "{cap}"
                            }
                        }
                    }
                }
            }

            // Error display
            if let Some(err) = &props.plugin.error {
                div {
                    style: "font-size: var(--text-2xs); padding: 6px 10px; border-radius: var(--radius-sm); background: var(--accentSubtle); color: var(--error); border: 1px solid var(--error);",
                    "{err}"
                }
            }
        }
    }
}
