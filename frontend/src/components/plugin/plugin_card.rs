use super::plugin_event_bus::PluginEntry;
use crate::tauri_bridge;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PluginCardProps {
    pub plugin: PluginEntry,
}

#[component]
pub fn PluginCard(props: PluginCardProps) -> Element {
    let (status_color, status_label) = if props.plugin.error.is_some() {
        ("var(--error)", "Error".to_string())
    } else if props.plugin.enabled {
        ("var(--success)", "Active".to_string())
    } else {
        ("var(--textDim)", "Inactive".to_string())
    };

    let title_color = if props.plugin.enabled {
        "var(--accent)"
    } else {
        "var(--text)"
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

    rsx! {
        div {
            class: "card lit-sweep",
            style: "border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); opacity: {opacity}; display: flex; flex-direction: column; gap: 10px;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",

                div {
                    style: "display: flex; align-items: center; gap: 8px;",

                    div {
                        style: "width: 28px; height: 28px; border-radius: var(--radius-md); background: var(--accentSubtle); display: flex; align-items: center; justify-content: center; font-family: var(--font-display); font-size: 14px; font-weight: 600; color: var(--accent); flex-shrink: 0; border: 1px solid var(--border);",
                        "{initial}"
                    }

                    div {
                        span {
                            style: "font-size: var(--text-sm); font-weight: 600; color: {title_color}; font-family: var(--font-ui); display: block;",
                            "{props.plugin.name}"
                        }
                        span {
                            style: "font-size: var(--text-2xs); color: var(--textDim);",
                            "v{props.plugin.version}"
                        }
                    }
                }

                // Toggle
                button {
                    class: "{toggle_class}",
                    onclick: move |_| {
                        let plugin_id = props.plugin.id.clone();
                        let currently_enabled = props.plugin.enabled;
                        spawn(async move {
                            if currently_enabled {
                                let _ = tauri_bridge::plugin_disable(&plugin_id).await;
                            } else {
                                let _ = tauri_bridge::plugin_enable(&plugin_id).await;
                            }
                        });
                    },
                    "{toggle_label}"
                }
            }

            // Status
            div {
                style: "display: flex; align-items: center; gap: 6px;",
                div {
                    style: "width: 6px; height: 6px; border-radius: var(--radius-pill); background: {status_color}; flex-shrink: 0;",
                }
                span {
                    style: "font-size: var(--text-2xs); padding: 1px 7px; border-radius: var(--radius-pill); background: color-mix(in srgb, {status_color} 12%, transparent); border: 1px solid color-mix(in srgb, {status_color} 32%, transparent); color: {status_color}; font-weight: 500;",
                    "{status_label}"
                }
            }

            // Error display
            if let Some(err) = &props.plugin.error {
                div {
                    style: "font-size: var(--text-2xs); padding: 6px 10px; border-radius: var(--radius-sm); background: color-mix(in srgb, var(--error) 12%, transparent); color: var(--error); border: 1px solid color-mix(in srgb, var(--error) 32%, transparent);",
                    "{err}"
                }
            }

            // Plugin ID
            div {
                style: "font-size: var(--text-2xs); color: var(--textDim); font-family: var(--font-mono);",
                "{props.plugin.id}"
            }
        }
    }
}
