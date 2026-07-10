use crate::components::shared::illustration::{EmptyArt, EmptyState};
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
    rsx! {
        div {
            class: "plugin-dashboard pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; flex: 1; overflow-y: auto; overflow-x: hidden; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md);",

            // Section header — accent + font-display
            div {
                style: "display: flex; align-items: center; gap: 10px; padding: 14px 18px; border-bottom: 1px solid var(--border);",
                h2 {
                    style: "margin: 0; font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; letter-spacing: 0.04em; color: var(--accent);",
                    "Plugins"
                }
            }

            EmptyState {
                kind: EmptyArt::Plugins,
                title: "No plugins".to_string(),
                hint: Some("Plugin management coming soon. Install plugin will appear here.".to_string()),
            }
        }
    }
}
