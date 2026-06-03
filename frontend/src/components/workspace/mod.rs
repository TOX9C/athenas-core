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
                        style: "flex: 1; display: flex; align-items: center; justify-content: center; text-align: center; color: var(--textDim);",
                        div {
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
