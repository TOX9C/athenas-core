use super::browser_panel::RightBrowserPanel;
use super::editor_panel::RightEditorPanel;
use super::skills_panel::SkillsPanel;
use crate::components::athena::athena_panel::{AthenaPanel, AthenaPanelMode};
use crate::components::shared::icon::{IconColumn, IconFile, IconGlobe, IconTerminal};
use crate::stores::panel_manager::{use_panel_manager_store, RightPanel};
use crate::stores::ui::{use_ui_store, Panel};
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
            "var(--textMuted)"
        };
        let border = if is_active {
            "2px solid var(--accent)"
        } else {
            "2px solid transparent"
        };
        let surface_border = if is_active {
            "border-bottom: 1px solid var(--border);"
        } else {
            "border-bottom: 1px solid transparent;"
        };
        format!(
            "display: flex; align-items: center; justify-content: center; gap: 6px; padding: 6px 14px; flex: 1; border-radius: 0; border: none; border-bottom: {border}; {surface_border} font-family: var(--font-ui); font-size: var(--text-xs); font-weight: 500; cursor: pointer; background: transparent; color: {fg}; letter-spacing: 0.04em; transition: color var(--dur-fast) var(--ease), box-shadow var(--dur-fast) var(--ease);"
        )
    };

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; background: var(--bgSecondary); border: 1px solid var(--border); border-left: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden;",

            // Tab bar
            div {
                style: "display: flex; align-items: center; gap: 0; padding: 0 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                button {
                    class: "lit-sweep",
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
                    class: "lit-sweep",
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
                    class: "lit-sweep",
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
                    class: "lit-sweep",
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
                    // The embedded browser is a single shared surface. When it
                    // is popped out to the main content area
                    // (`ui_state.panel == Panel::Browser`), the sidebar yields
                    // ownership and shows a dock hint instead of mounting a
                    // second surface.
                    RightPanel::Browser => if ui_state.read().panel == Panel::Browser {
                        rsx! {
                            div {
                                style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; padding: 24px; text-align: center; color: var(--textMuted);",
                                span {
                                    style: "color: var(--textMuted); font-family: var(--font-ui); font-size: var(--text-xs); letter-spacing: 0.04em;",
                                    "Browser Relocated"
                                }
                                div {
                                    style: "font-family: var(--font-display); font-size: 14px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                                    "Browser is in the main area"
                                }
                                div {
                                    style: "font-size: 11px; max-width: 220px; color: var(--textMuted);",
                                    "Use the dock button in the browser toolbar to bring it back here."
                                }
                            }
                        }
                    } else {
                        rsx! { RightBrowserPanel {} }
                    },
                    RightPanel::Assistant => rsx! { AthenaPanel { mode: AthenaPanelMode::Compact } },
                    // The editor has one shared surface. When the main
                    // content area owns it (`Panel::Editor`), avoid mounting
                    // a second editor against the same store in the sidebar.
                    RightPanel::Editor => if ui_state.read().panel == Panel::Editor {
                        rsx! {
                            div {
                                style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; padding: 24px; text-align: center; color: var(--textMuted);",
                                span {
                                    style: "color: var(--textMuted); font-family: var(--font-ui); font-size: var(--text-xs); letter-spacing: 0.04em;",
                                    "Editor Relocated"
                                }
                                div {
                                    style: "font-family: var(--font-display); font-size: 14px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                                    "Editor is in the main area"
                                }
                                div {
                                    style: "font-size: 11px; max-width: 220px; color: var(--textMuted);",
                                    "Use the panel shortcut or navigation to bring it back here."
                                }
                            }
                        }
                    } else {
                        rsx! { RightEditorPanel {} }
                    },
                    RightPanel::Skills => rsx! { SkillsPanel {} },
                    RightPanel::None => rsx! {},
                }
            }
        }
    }
}
