pub mod agent_info_poller;
pub mod grid_template;
pub mod new_space_modal;
pub mod terminal_grid;
pub mod workspace_tab;
pub mod workspace_tabs;

#[cfg(feature = "xterm")]
pub mod xterm_mount;

// Re-export panel
use super::workspace::workspace_tabs::WorkspaceTabs;
use crate::components::workspace::terminal_grid::WorkspaceGrid;
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

            // Content area — stretch, not center, so the child grid
            // fills the full viewport (align-items:center shrinks it).
            div {
                style: "flex: 1; display: flex; align-items: stretch; justify-content: stretch; overflow: hidden; min-width: 0; min-height: 0;",

                if state.spaces.is_empty() {
                    div {
                        class: "animate-rise",
                        style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 18px; text-align: center;",
                        crate::components::shared::illustration::OwlMark { size: Some(48) }
                        div {
                            div {
                                style: "font-family: var(--font-display); font-size: 22px; font-weight: 600; color: var(--text);",
                                "No workspace"
                            }
                            span { style: "font-size: 13px; margin-top: 4px; display: block; color: var(--textMuted);", "Create a workspace to get started." }
                        }
                        button {
                            class: "btn-primary",
                            // TODO: open NewSpaceModal
                            crate::components::shared::icon::IconPlus { size: Some(15), color: Some("currentColor".to_string()) }
                            "New Space"
                        }
                    }
                } else {
                    WorkspaceGrid {
                        active_space: state.active_space_id.clone().and_then(|id| state.spaces.iter().find(|s| s.id == id).cloned()),
                        active_space_id: state.active_space_id.clone(),
                    }
                }
            }
        }
    }
}
