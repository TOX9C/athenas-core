pub mod kanban_board;
pub mod kanban_card;
pub mod kanban_column;

// Re-export panel
use super::kanban::kanban_board::KanbanBoard;
use dioxus::prelude::*;

#[component]
pub fn KanbanPanel() -> Element {
    rsx! {
        div {
            class: "kanban-panel",
            style: "height: 100%; display: flex; flex-direction: column;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary);",
                span {
                    style: "font-size: 13px; font-weight: 600; color: var(--text);",
                    "Kanban"
                }
            }

            KanbanBoard {}
        }
    }
}
