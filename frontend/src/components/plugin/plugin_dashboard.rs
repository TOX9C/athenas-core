use super::plugin_card::PluginCard;
use crate::stores::notification::use_notification_store;
use dioxus::prelude::*;

/// Plugin data model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PluginData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub status: String,
    pub enabled: bool,
    pub agent_count: u32,
    pub capabilities: Vec<String>,
    pub error: Option<String>,
}

#[component]
pub fn PluginDashboard() -> Element {
    let _notifications = use_notification_store();
    let mut search = use_signal(String::new);
    let mut status_filter = use_signal(|| "all".to_string());

    // Derive plugins from notification store — plugin events are tracked there.
    // In a full implementation this would call Tauri IPC: plugin_list().
    let plugins: Vec<PluginData> = Vec::new();

    let filtered: Vec<PluginData> = plugins
        .iter()
        .filter(|p| {
            if search().is_empty() {
                true
            } else {
                let q = search().to_lowercase();
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
                    || p.id.to_lowercase().contains(&q)
            }
        })
        .filter(|p| match status_filter().as_str() {
            "active" => p.enabled && p.error.is_none(),
            "error" => p.error.is_some(),
            "inactive" => !p.enabled,
            _ => true,
        })
        .cloned()
        .collect();

    rsx! {
        div {
            class: "plugin-dashboard",
            style: "display: flex; flex-direction: column; height: 100%; background: var(--bg); color: var(--text);",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary);",

                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    span {
                        style: "font-size: 13px; font-weight: 600; color: var(--text);",
                        "Plugins"
                    }
                    span {
                        style: "font-size: 9px; padding: 1px 5px; border-radius: 9999px; background: var(--bgTertiary); color: var(--textDim);",
                        "{plugins.len()}"
                    }
                }

                button {
                    style: "padding: 4px 8px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 10px; font-weight: 600;",
                    onclick: move |_| {
                        // TODO: refresh plugins via Tauri IPC
                    },
                    "REFRESH"
                }
            }

            // Search + filter
            div {
                style: "display: flex; gap: 6px; padding: 6px 12px; border-bottom: 1px solid var(--border);",

                div {
                    style: "flex: 1; display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-radius: 6px; background: var(--bgTertiary); border: 1px solid var(--border);",
                    input {
                        style: "flex: 1; background: transparent; border: none; outline: none; color: var(--text); font-size: 11px;",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                        placeholder: "Search plugins..."
                    }
                }

                for filter in ["all", "active", "error", "inactive"] {
                    {
                        let is_active = status_filter() == filter;
                        let bg = if is_active { "var(--accent)" } else { "transparent" };
                        let color = if is_active { "#0b0e13" } else { "var(--textDim)" };
                        let filter_owned = filter.to_string();
                        rsx! {
                            button {
                                key: "{filter}",
                                style: "padding: 3px 8px; border-radius: 4px; border: none; background: {bg}; color: {color}; cursor: pointer; font-size: 9px; text-transform: capitalize;",
                                onclick: move |_| status_filter.set(filter_owned.clone()),
                                "{filter}"
                            }
                        }
                    }
                }
            }

            // Plugin grid
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px;",

                if filtered.is_empty() {
                    div {
                        style: "text-align: center; padding: 32px; color: var(--textDim); font-size: 11px;",
                        "No plugins installed"
                    }
                } else {
                    div {
                        style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 8px;",
                        for plugin in filtered.iter() {
                            PluginCard { key: "{plugin.id}", plugin: plugin.clone() }
                        }
                    }
                }
            }
        }
    }
}
