use super::plugin_event_bus::PluginEntry;
use crate::tauri_bridge;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PluginCardProps {
    pub plugin: PluginEntry,
}

#[component]
pub fn PluginCard(props: PluginCardProps) -> Element {
    let status_label = if props.plugin.enabled {
        "Active".to_string()
    } else if props.plugin.error.is_some() {
        "Error".to_string()
    } else {
        "Inactive".to_string()
    };
    let status_color = if props.plugin.enabled {
        "var(--success)"
    } else if props.plugin.error.is_some() {
        "var(--error)"
    } else {
        "var(--textDim)"
    };

    let title_color = if props.plugin.enabled { "var(--accent)" } else { "var(--text)" };
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
            class: "card",
            style: "padding: 14px; opacity: {opacity}; display: flex; flex-direction: column; gap: 10px;",

            // Header: avatar + name + version + toggle
            div {
                style: "display: flex; align-items: center; gap: 10px;",

                // Avatar
                div {
                    style: "width: 36px; height: 36px; border-radius: var(--radius-sm); background: var(--accentSubtle); display: flex; align-items: center; justify-content: center; font-family: var(--font-display); font-size: var(--text-md); font-weight: 700; color: var(--accent); flex-shrink: 0;",
                    "{initial}"
                }

                // Name + version
                div {
                    style: "flex: 1; min-width: 0;",
                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: {title_color}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{props.plugin.name}"
                    }
                    div {
                        style: "font-size: var(--text-xs); color: var(--textMuted);",
                        "v{props.plugin.version}"
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
                            // The PluginEventBus will react to the backend's
                            // enable/disable event and update the store.
                        });
                    },
                    "{toggle_label}"
                }
            }

            // Status indicator
            div {
                style: "display: flex; align-items: center; gap: 6px;",
                span {
                    style: "width: 6px; height: 6px; border-radius: 50%; background: {status_color}; flex-shrink: 0;",
                }
                span {
                    style: "font-size: var(--text-xs); color: var(--textMuted);",
                    "{status_label}"
                }
            }

            // Error message if any
            if let Some(ref err) = props.plugin.error {
                div {
                    style: "font-size: var(--text-xs); color: var(--error); padding: 6px 8px; border-radius: var(--radius-sm); background: rgba(var(--error-rgb), 0.08);",
                    "{err}"
                }
            }

            // Plugin ID
            div {
                style: "font-size: var(--text-xs); color: var(--textDim); font-family: var(--font-mono);",
                "{props.plugin.id}"
            }
        }
    }
}
