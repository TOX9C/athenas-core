use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Payload carried during a drag operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DragPayload {
    GridPane {
        space_id: String,
        source_slot: usize,
        pane_id: String,
        pane_label: String,
    },
    Agent {
        pane_id: String,
        agent_type: String,
        label: String,
    },
}

/// Global drag-and-drop state.
///
/// Shared context used by grid cells (drop targets) and pill drag handles.
#[derive(Default)]
pub struct DragLayer {
    pub active: Signal<Option<DragPayload>>,
    pub cursor_xy: Signal<(i32, i32)>,
    pub hovered_cell: Signal<Option<usize>>, // slot_index of the hovered drop target
}

impl DragLayer {
    pub fn new() -> Self {
        Self {
            active: Signal::new(None),
            cursor_xy: Signal::new((0, 0)),
            hovered_cell: Signal::new(None),
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.active.read().is_some()
    }

    pub fn set_active(&mut self, payload: Option<DragPayload>) {
        self.active.set(payload);
    }

    pub fn set_hovered(&mut self, slot: Option<usize>) {
        self.hovered_cell.set(slot);
    }

    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.cursor_xy.set((x, y));
    }

    pub fn clear(&mut self) {
        self.active.set(None);
        self.hovered_cell.set(None);
    }
}
