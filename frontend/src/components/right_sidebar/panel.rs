use super::browser_panel::RightBrowserPanel;
use super::skills_panel::SkillsPanel;
use crate::components::athena::athena_panel::AthenaPanel;
use crate::stores::panel_manager::{use_panel_manager_store, RightPanel};
use dioxus::prelude::*;

#[component]
pub fn RightSidebar() -> Element {
    let mut panel_state = use_panel_manager_store();
    let width = panel_state.read().right_panel_width_percent;
    let active = panel_state.read().active_right_panel;

    let tab_btn = |is_active: bool| -> String {
        let fg = if is_active {
            "var(--accent)"
        } else {
            "var(--textDim)"
        };
        let border = if is_active {
            "1px solid var(--accent)"
        } else {
            "1px solid transparent"
        };
        format!(
            "padding: 6px 14px; border-radius: 2px; border: none; border-bottom: {border}; font-size: 11px; font-weight: 500; cursor: pointer; background: transparent; color: {fg}; transition: color 0.2s ease;"
        )
    };

    rsx! {
        div {
            style: "width: {width}%; border-left: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; min-width: 0; background: var(--bg); overflow: hidden;",

            // Tab bar
            div {
                style: "display: flex; align-items: center; gap: 0; padding: 0 8px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                button {
                    style: "{tab_btn(active == RightPanel::Browser)}",
                    onclick: move |_| panel_state.write().toggle_right_panel(RightPanel::Browser),
                    "Browser"
                }

                button {
                    style: "{tab_btn(active == RightPanel::Assistant)}",
                    onclick: move |_| panel_state.write().toggle_right_panel(RightPanel::Assistant),
                    "Assistant"
                }

                button {
                    style: "{tab_btn(active == RightPanel::Skills)}",
                    onclick: move |_| panel_state.write().toggle_right_panel(RightPanel::Skills),
                    "Skills"
                }

                div { style: "flex: 1;" }

                button {
                    style: "padding: 6px 8px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 13px; line-height: 1;",
                    onclick: move |_| panel_state.write().close_right_panel(),
                    "x"
                }
            }

            // Content
            div {
                style: "flex: 1; min-height: 0; overflow: hidden;",

                match active {
                    RightPanel::Browser => rsx! { RightBrowserPanel {} },
                    RightPanel::Assistant => rsx! { AthenaPanel {} },
                    RightPanel::Skills => rsx! { SkillsPanel {} },
                    RightPanel::None => rsx! {},
                }
            }
        }
    }
}
