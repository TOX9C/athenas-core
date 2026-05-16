pub mod command_block;
pub mod command_block_list;
pub mod pane_header;
pub mod terminal_grid;
pub mod terminal_pane;

// Re-export main panel
use super::terminal::terminal_grid::TerminalGrid;
use dioxus::prelude::*;

#[component]
pub fn TerminalPanel() -> Element {
    rsx! {
        div {
            class: "terminal-panel",
            style: "height: 100%; display: flex; flex-direction: column; background: var(--bg); color: var(--text);",

            // Toolbar
            div {
                style: "display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                button {
                    style: "padding: 4px 10px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--text); cursor: pointer; font-size: 11px;",
                    onclick: move |_| {
                        // TODO: create new terminal pane via Tauri IPC
                    },
                    "+"
                }

                span {
                    style: "font-size: 13px; font-weight: 500; color: var(--textMuted);",
                    "Terminals"
                }
            }

            // Grid
            TerminalGrid {}
        }
    }
}
