use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Payload carried during a drag operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DragPayload {
    GridPane {
        space_id: String,
        source_slot: usize,
        #[serde(default)]
        pane_id: String,
        #[serde(default)]
        pane_label: String,
        #[serde(default)]
        agent_type: String,
    },
    Agent {
        #[serde(default)]
        pane_id: String,
        #[serde(default)]
        agent_type: String,
        #[serde(default)]
        label: String,
    },
}

/// Global drag-and-drop state.
///
/// Shared context used by grid cells (drop targets) and pill drag handles.
#[derive(Default)]
pub struct DragLayer {
    pub active: Signal<Option<DragPayload>>,
}

impl DragLayer {
    pub fn new() -> Self {
        Self {
            active: Signal::new(None),
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.active.read().is_some()
    }

    pub fn set_active(&mut self, payload: Option<DragPayload>) {
        self.active.set(payload);
    }

    pub fn clear(&mut self) {
        self.active.set(None);
    }
}
