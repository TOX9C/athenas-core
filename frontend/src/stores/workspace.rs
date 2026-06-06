use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::tauri_bridge::store_get as kv_get;
use crate::tauri_bridge::store_set as kv_set;

// ---------------------------------------------------------------------------
// Re-exports from types::workspace (canonical definitions with strum/serde)
// ---------------------------------------------------------------------------

pub use crate::types::workspace::{AgentType, GridTemplate, PaneConfig, Space};

/// Key used in KeyValueStore for workspace persistence.
const WORKSPACES_KEY: &str = "workspaces";

/// Select the smallest grid template that can hold the given pane count.
pub fn grid_for_pane_count(count: usize) -> GridTemplate {
    if count <= 1 {
        GridTemplate::X1x1
    } else if count <= 2 {
        GridTemplate::X1x2
    } else if count <= 4 {
        GridTemplate::X2x2
    } else if count <= 6 {
        GridTemplate::X2x3
    } else if count <= 9 {
        GridTemplate::X3x3
    } else if count <= 12 {
        GridTemplate::X3x4
    } else {
        GridTemplate::X4x4
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global workspace state.
#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub spaces: Vec<Space>,
    pub active_space_id: Option<String>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            spaces: Vec::new(),
            active_space_id: None,
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn set_active_space(&mut self, id: impl Into<String>) {
        self.active_space_id = Some(id.into());
        self.save();
    }

    pub fn add_space(&mut self, space: Space) {
        let id = space.id.clone();
        self.spaces.push(space);
        self.active_space_id = Some(id);
        self.save();
    }

    pub fn remove_space(&mut self, id: &str) {
        self.spaces.retain(|s| s.id != id);
        if self.active_space_id.as_deref() == Some(id) {
            self.active_space_id = self.spaces.last().map(|s| s.id.clone());
        }
        self.save();
    }

    pub fn update_space(&mut self, id: &str, f: impl FnOnce(&mut Space)) {
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == id) {
            f(space);
        }
        self.save();
    }

    pub fn add_pane_to_space(&mut self, space_id: &str, pane: PaneConfig) {
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) {
            space.panes.push(pane);
            space.grid = grid_for_pane_count(space.panes.len());
        }
        self.save();
    }

    pub fn remove_pane_from_space(&mut self, space_id: &str, pane_id: &str) {
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) {
            space.panes.retain(|p| p.id != pane_id);
            space.grid = grid_for_pane_count(space.panes.len());
        }
        self.save();
    }

    pub fn set_spaces(&mut self, spaces: Vec<Space>) {
        self.spaces = spaces;
    }

    /// Persist the current workspace state to the backend KeyValueStore.
    /// Call this after every mutation that changes workspace layout or panes.
    pub fn save(&self) {
        let json = match serde_json::to_string(self) {
            Ok(j) => j,
            Err(e) => {
                web_sys::console::error_1(
                    &format!("[WorkspaceState] serialize error: {}", e).into(),
                );
                return;
            }
        };
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = kv_set(WORKSPACES_KEY, &json).await {
                web_sys::console::error_1(
                    &format!("[WorkspaceState] store_set error: {:?}", e).into(),
                );
            }
        });
    }

    /// Load workspace state from the backend KeyValueStore.
    /// Returns the deserialized state, or an empty one if nothing is saved.
    pub async fn load() -> Self {
        match kv_get(WORKSPACES_KEY).await {
            Ok(json) => {
                if json.trim().is_empty() {
                    return Self::new();
                }
                match serde_json::from_str(&json) {
                    Ok(state) => state,
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("[WorkspaceState] deserialize error: {}", e).into(),
                        );
                        Self::new()
                    }
                }
            }
            Err(e) => {
                web_sys::console::error_1(
                    &format!("[WorkspaceState] store_get error: {:?}", e).into(),
                );
                Self::new()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the workspace signal from the Dioxus context.
pub fn use_workspace_store() -> Signal<WorkspaceState> {
    use_context::<Signal<WorkspaceState>>()
}

/// Initialize the workspace store as a context provider.
pub fn provide_workspace_store() {
    use_context_provider(|| Signal::new(WorkspaceState::new()));
}
