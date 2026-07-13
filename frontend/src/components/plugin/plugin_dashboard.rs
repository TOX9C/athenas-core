use super::plugin_card::PluginCard;
use super::plugin_event_bus::use_plugin_bus_store;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[component]
pub fn PluginDashboard() -> Element {
    let plugin_bus = use_plugin_bus_store();
    let mut loaded = use_signal(|| false);

    // Fetch plugin list from backend on mount.
    use_effect(move || {
        if loaded() {
            return;
        }
        loaded.set(true);
        let mut bus = plugin_bus;
        spawn(async move {
            match tauri_bridge::plugin_list().await {
                Ok(json) => {
                    if let Ok(arr) = serde_json::from_str::<serde_json::Value>(&json) {
                        if let Some(items) = arr.as_array() {
                            for item in items {
                                let id = item
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = item
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Unknown")
                                    .to_string();
                                let version = item
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("0.0.0")
                                    .to_string();
                                let enabled = item
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s == "enabled")
                                    .unwrap_or(false);
                                bus.write().upsert_plugin(id, name, version, enabled);
                            }
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("[PluginDashboard] plugin_list failed: {:?}", e).into(),
                    );
                }
            }
        });
    });

    let plugins = plugin_bus.read().plugins.clone();

    rsx! {
        div {
            class: "plugin-dashboard pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; flex: 1; overflow-y: auto; overflow-x: hidden; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md);",

            // Section header
            div {
                style: "display: flex; align-items: center; gap: 10px; padding: 14px 18px; border-bottom: 1px solid var(--border);",
                h2 {
                    style: "margin: 0; font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; letter-spacing: 0.04em; color: var(--accent);",
                    "Plugins"
                }
                if !plugins.is_empty() {
                    span {
                        class: "badge",
                        "{plugins.len()}"
                    }
                }
            }

            // Plugin cards
            if plugins.is_empty() {
                EmptyState {
                    kind: EmptyArt::Plugins,
                    title: "No plugins".to_string(),
                    hint: Some("Install plugins via the plugin registry. Detected plugins will appear here.".to_string()),
                }
            } else {
                div {
                    style: "display: flex; flex-direction: column; gap: 8px; padding: 12px;",

                    for plugin in plugins.iter() {
                        PluginCard { key: "{plugin.id}", plugin: plugin.clone() }
                    }
                }
            }
        }
    }
}
