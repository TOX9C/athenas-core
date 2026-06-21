use super::browser_panel::RightBrowserPanel;
use super::editor_panel::RightEditorPanel;
use super::skills_panel::SkillsPanel;
use crate::components::athena::athena_panel::{AthenaPanel, AthenaPanelMode};
use crate::components::shared::icon::{IconColumn, IconFile, IconGlobe, IconTerminal};
use crate::stores::panel_manager::{use_panel_manager_store, RightPanel};
use crate::stores::ui::use_ui_store;
use dioxus::prelude::*;

#[component]
pub fn RightSidebar() -> Element {
    let mut panel_state = use_panel_manager_store();
    let mut ui_state = use_ui_store();
    let active = panel_state.read().active_right_panel;

    let tab_btn = |is_active: bool| -> String {
        let fg = if is_active {
            "var(--accent)"
        } else {
            "var(--textDim)"
        };
        let border = if is_active {
            "2px solid var(--accent)"
        } else {
            "2px solid transparent"
        };
        format!(
            "display: flex; align-items: center; justify-content: center; gap: 6px; padding: 6px 14px; flex: 1; border-radius: 0; border: none; border-bottom: {border}; font-family: var(--font-ui); font-size: var(--text-xs); font-weight: 500; cursor: pointer; background: transparent; color: {fg}; transition: color var(--dur-fast) var(--ease);"
        )
    };

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; background: var(--bg); overflow: hidden;",

            // Tab bar
            div {
                style: "display: flex; align-items: center; gap: 0; padding: 0 8px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                button {
                    style: "{tab_btn(active == RightPanel::Browser)}",
                    onclick: move |_| {
                        let sidebar_open = ui_state.read().right_sidebar_open;
                        let should_be_open = panel_state.write().toggle_right_panel(RightPanel::Browser, sidebar_open);
                        ui_state.write().right_sidebar_open = should_be_open;
                    },
                    IconGlobe { size: Some(13), color: Some("currentColor".to_string()) }
                    "Browser"
                }

                button {
                    style: "{tab_btn(active == RightPanel::Assistant)}",
                    onclick: move |_| {
                        let sidebar_open = ui_state.read().right_sidebar_open;
                        let should_be_open = panel_state.write().toggle_right_panel(RightPanel::Assistant, sidebar_open);
                        ui_state.write().right_sidebar_open = should_be_open;
                    },
                    IconTerminal { size: Some(13), color: Some("currentColor".to_string()) }
                    "Athena"
                }

                button {
                    style: "{tab_btn(active == RightPanel::Editor)}",
                    onclick: move |_| {
                        let sidebar_open = ui_state.read().right_sidebar_open;
                        let should_be_open = panel_state.write().toggle_right_panel(RightPanel::Editor, sidebar_open);
                        ui_state.write().right_sidebar_open = should_be_open;
                    },
                    IconFile { size: Some(13), color: Some("currentColor".to_string()) }
                    "Editor"
                }

                button {
                    style: "{tab_btn(active == RightPanel::Skills)}",
                    onclick: move |_| {
                        let sidebar_open = ui_state.read().right_sidebar_open;
                        let should_be_open = panel_state.write().toggle_right_panel(RightPanel::Skills, sidebar_open);
                        ui_state.write().right_sidebar_open = should_be_open;
                    },
                    IconColumn { size: Some(13), color: Some("currentColor".to_string()) }
                    "Skills"
                }
            }

            // Content
            div {
                style: "flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column;",

                match active {
                    RightPanel::Browser => rsx! { RightBrowserPanel {} },
                    RightPanel::Assistant => rsx! { AthenaPanel { mode: AthenaPanelMode::Compact } },
                    RightPanel::Editor => rsx! { RightEditorPanel {} },
                    RightPanel::Skills => rsx! { SkillsPanel {} },
                    RightPanel::None => rsx! {},
                }
            }
        }
    }
}
