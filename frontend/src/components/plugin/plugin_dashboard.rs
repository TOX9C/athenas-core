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
            class: "plugin-dashboard",
            style: "display: flex; flex-direction: column; height: 100%; flex: 1; overflow-y: auto;",

            EmptyState {
                kind: EmptyArt::Plugins,
                title: "No plugins".to_string(),
                hint: Some("Plugin management coming soon. Install plugin will appear here.".to_string()),
            }
        }
    }
}
