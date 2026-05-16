pub mod grid_template;
pub mod new_space_modal;
pub mod workspace_tab;
pub mod workspace_tabs;

// Re-export panel
use super::workspace::workspace_tabs::WorkspaceTabs;
use crate::stores::workspace::use_workspace_store;
use dioxus::prelude::*;

#[component]
pub fn WorkspacePanel() -> Element {
    let workspace_state = use_workspace_store();
    let state = workspace_state.read();

    rsx! {
        div {
            class: "workspace-panel",
            style: "height: 100%; display: flex; flex-direction: column; background: var(--bg); color: var(--text);",

            WorkspaceTabs { on_new_space: move |_| {} }

            // Content area
            div {
                style: "flex: 1; display: flex; align-items: center; justify-content: center; overflow: auto;",

                if state.spaces.is_empty() {
                    div {
                        style: "text-align: center; color: var(--textDim);",
                        div {
                            style: "width: 40px; height: 40px; border-radius: 8px; background: var(--bgTertiary); display: flex; align-items: center; justify-content: center; margin: 0 auto; opacity: 0.4;",
                            span { style: "font-size: 16px; font-weight: 700; color: var(--textMuted);", "W" }
                        }
                        span { style: "font-size: 12px; margin-top: 8px; display: block;", "Create a workspace to get started" }
                        button {
                            style: "margin-top: 12px; padding: 8px 16px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px;",
                            // TODO: open NewSpaceModal
                            "+ New Space"
                        }
                    }
                } else {
                    div {
                        style: "padding: 16px; width: 100%;",
                        if let Some(active_id) = &state.active_space_id {
                            if let Some(space) = state.spaces.iter().find(|s| &s.id == active_id) {
                                div {
                                    style: "font-size: 14px; font-weight: 600; color: var(--text);",
                                    "{space.name}"
                                }
                                div {
                                    style: "font-size: 11px; color: var(--textDim); margin-top: 4px;",
                                    "{space.dir}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
