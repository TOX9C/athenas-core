use crate::components::agents::AgentInspector;
use crate::components::athena::AthenaPanel;
use crate::components::browser::BrowserPanel;
use crate::components::editor::EditorPanel;
use crate::components::kanban::KanbanPanel;
use crate::components::notifications::NotificationPanel;
use crate::components::plugin::PluginDashboard;
use crate::components::settings::SettingsPanel;
use crate::components::swarm::SwarmPanel;
use crate::components::terminal::TerminalPanel;
use crate::components::workspace::WorkspacePanel;
use crate::stores::ui::{use_ui_store, Panel};
use dioxus::prelude::*;

#[component]
pub fn PanelView() -> Element {
    let ui_state = use_ui_store();
    let panel = ui_state.read().panel;

    rsx! {
        div {
            class: "panel-view",
            style: "flex-grow: 1; height: 100%; overflow: hidden; background: #11111b; color: #c8d5e8;",
            match panel {
                Panel::Chat => rsx! { AthenaPanel {} },
                Panel::Terminal => rsx! { TerminalPanel {} },
                Panel::Workspace => rsx! { WorkspacePanel {} },
                Panel::Editor => rsx! { EditorPanel {} },
                Panel::Plugin => rsx! { PluginDashboard {} },
                Panel::Swarm => rsx! { SwarmPanel {} },
                Panel::Browser => rsx! { BrowserPanel {} },
                Panel::Kanban => rsx! { KanbanPanel {} },
                Panel::Notifications => rsx! { NotificationPanel {} },
                Panel::Agents => rsx! { AgentInspector {} },
                Panel::Settings => rsx! { SettingsPanel {} },
            }
        }
    }
}
