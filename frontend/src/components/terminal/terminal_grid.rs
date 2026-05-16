use super::terminal_pane::TerminalPane;
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{use_workspace_store, GridTemplate, Space};
use dioxus::prelude::*;

#[component]
pub fn TerminalGrid() -> Element {
    let workspace_state = use_workspace_store();
    let ui_state = use_ui_store();

    let fullscreen_pane_id = ui_state.read().fullscreen_pane_id.clone();

    let state = workspace_state.read();
    let active_space: Option<Space> = state
        .active_space_id
        .as_ref()
        .and_then(|id| state.spaces.iter().find(|s| &s.id == id))
        .cloned();
    drop(state);

    // Fullscreen mode: render only the fullscreen pane at 100%.
    if let Some(ref fs_id) = fullscreen_pane_id {
        if let Some(ref space) = active_space {
            if let Some(pane) = space.panes.iter().find(|p| p.id == *fs_id) {
                return rsx! {
                    div {
                        class: "terminal-grid",
                        style: "flex: 1; overflow: hidden; padding: 2px; background: var(--bgSecondary);",

                        TerminalPane {
                            key: "{pane.id}",
                            pane_id: pane.id.clone(),
                            agent_type: format!("{:?}", pane.agent_type)
                        }
                    }
                };
            }
        }
    }

    let (cols, rows) = match active_space.as_ref().map(|s| s.grid) {
        Some(GridTemplate::X1x1) => (1, 1),
        Some(GridTemplate::X1x2) => (2, 1),
        Some(GridTemplate::X2x2) => (2, 2),
        Some(GridTemplate::X2x3) => (3, 2),
        Some(GridTemplate::X3x3) => (3, 3),
        Some(GridTemplate::X3x4) => (4, 3),
        Some(GridTemplate::X4x4) => (4, 4),
        None => (1, 1),
    };

    let grid_style = format!(
        "display: grid; grid-template-columns: repeat({}, 1fr); grid-template-rows: repeat({}, 1fr); flex: 1; gap: 2px; padding: 2px; overflow: hidden; background: var(--bgSecondary);",
        cols, rows
    );

    rsx! {
        div {
            class: "terminal-grid",
            style: "{grid_style}",

            if let Some(space) = active_space {
                for pane in space.panes.iter() {
                    TerminalPane {
                        key: "{pane.id}",
                        pane_id: pane.id.clone(),
                        agent_type: format!("{:?}", pane.agent_type)
                    }
                }
            } else {
                div {
                    style: "grid-column: 1 / -1; grid-row: 1 / -1; display: flex; align-items: center; justify-content: center; color: var(--textDim); font-size: 12px;",
                    "No active workspace. Create a space to get started."
                }
            }
        }
    }
}
