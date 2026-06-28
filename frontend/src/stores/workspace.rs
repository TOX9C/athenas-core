use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use wasm_bindgen::JsValue;

use crate::tauri_bridge::store_get as kv_get;
#[cfg(not(test))]
use crate::tauri_bridge::store_set as kv_set;

// ---------------------------------------------------------------------------
// Re-exports from types::workspace (canonical definitions with strum/serde)
// ---------------------------------------------------------------------------

pub use crate::types::workspace::{AgentType, GridTemplate, PaneConfig, Space};

/// Key used in KeyValueStore for workspace persistence.
const WORKSPACES_KEY: &str = "workspaces";

/// Monotonic generation counter for coalescing concurrent saves attempts.
static SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);

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

    pub fn add_pane_to_space(&mut self, space_id: &str, mut pane: PaneConfig) {
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) {
            let slot = space.panes.len();
            pane.slot_index = slot;
            space.panes.push(pane);
            space.grid = grid_for_pane_count(space.panes.len());
        }
        self.save();
    }

    pub fn remove_pane_from_space(&mut self, space_id: &str, pane_id: &str) {
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) {
            if let Some(idx) = space.panes.iter().position(|p| p.id == pane_id) {
                space.panes.remove(idx);
                for (i, pane) in space.panes.iter_mut().enumerate() {
                    pane.slot_index = i;
                }
            }
            space.grid = grid_for_pane_count(space.panes.len());
        }
        self.save();
    }

    /// Swap the slot_index of two panes in the given space.
    pub fn swap_pane_slots(&mut self, space_id: &str, a: usize, b: usize) {
        if a == b {
            #[cfg(not(test))]
            web_sys::console::warn_1(&"[swap_pane_slots] a == b, no-op".into());
            return;
        }
        let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) else {
            #[cfg(not(test))]
            web_sys::console::warn_1(&format!("[swap_pane_slots] space not found: {}", space_id).into());
            return;
        };
        let Some(pane_a_idx) = space.panes.iter().position(|p| p.slot_index == a) else {
            #[cfg(not(test))]
            web_sys::console::warn_1(&format!("[swap_pane_slots] source pane with slot_index {} not found in space {}", a, space_id).into());
            return;
        };
        let Some(pane_b_idx) = space.panes.iter().position(|p| p.slot_index == b) else {
            #[cfg(not(test))]
            web_sys::console::warn_1(&format!("[swap_pane_slots] target pane with slot_index {} not found in space {}", b, space_id).into());
            return;
        };
        let pane_a_id = space.panes[pane_a_idx].id.clone();
        let pane_b_id = space.panes[pane_b_idx].id.clone();
        for pane in &mut space.panes {
            if pane.id == pane_a_id {
                pane.slot_index = b;
            } else if pane.id == pane_b_id {
                pane.slot_index = a;
            }
        }
        self.save();
    }

    pub fn set_spaces(&mut self, spaces: Vec<Space>) {
        self.spaces = spaces;
        self.save();
    }

    /// Persist the current workspace state to the backend KeyValueStore.
    /// Call this after every mutation that changes workspace layout or panes.
    /// Saves are coalesced with a generation counter so that overlapping
    /// async writes never result in a stale state overwriting a newer one.
    pub fn save(&self) {
        #[cfg(not(test))]
        {
            let json = match serde_json::to_string(self) {
                Ok(j) => j,
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("[WorkspaceState] serialize error: {}", e).into(),
                    );
                    return;
                }
            };

            // Acquire a generation number for this save request.
            let my_gen = SAVE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

            wasm_bindgen_futures::spawn_local(async move {
                // Yield briefly so that any synchronously-spawned save()
                // calls have a chance to increment the global counter.
                let _ = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(
                    &JsValue::UNDEFINED,
                ))
                .await;

                // Only the latest generation should perform the write;
                // earlier saves self-cancel to prevent stale overwrites.
                if SAVE_GENERATION.load(Ordering::SeqCst) > my_gen {
                    return;
                }

                if let Err(e) = kv_set(WORKSPACES_KEY, &json).await {
                    web_sys::console::error_1(
                        &format!("[WorkspaceState] store_set error: {:?}", e).into(),
                    );
                }
            });
        }
    }

    /// Load workspace state from the backend KeyValueStore.
    /// Returns the deserialized state, or an empty one if nothing is saved.
    /// After loading, legacy workspaces (all panes with slot_index == 0)
    /// are re-indexed to their Vec position.
    pub async fn load() -> Self {
        match kv_get(WORKSPACES_KEY).await {
            Ok(json) => {
                if json.trim().is_empty() {
                    return Self::new();
                }
                match serde_json::from_str::<Self>(&json) {
                    Ok(mut state) => {
                        let mut reindexed = false;
                        for space in &mut state.spaces {
                            if space.panes.len() > 1
                                && space.panes.iter().all(|p| p.slot_index == 0)
                            {
                                for (i, pane) in space.panes.iter_mut().enumerate() {
                                    pane.slot_index = i;
                                }
                                reindexed = true;
                            }
                            // Also re-index if there are duplicate slot_index values
                            let unique_slots: std::collections::HashSet<usize> =
                                space.panes.iter().map(|p| p.slot_index).collect();
                            if unique_slots.len() != space.panes.len() {
                                for (i, pane) in space.panes.iter_mut().enumerate() {
                                    pane.slot_index = i;
                                }
                                reindexed = true;
                            }
                        }
                        if reindexed {
                            state.save();
                        }
                        state
                    }
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("[WorkspaceState] deserialize error: {}", e).into(),
                        );
                        Self::new()
                    }
                }
            }
            Err(_) => {
                // Key absent on first run — not an error.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pane(id: &str, slot_index: usize) -> PaneConfig {
        PaneConfig {
            id: id.to_string(),
            agent_type: AgentType::Claude,
            custom_cmd: None,
            custom_agent_id: None,
            label: None,
            bypass_mode: None,
            project_name: None,
            model_name: None,
            resume_id: None,
            resume_cmd: None,
            resume_dismissed: None,
            slot_index,
        }
    }

    #[test]
    fn swap_same_slot_is_noop() {
        let mut state = WorkspaceState::new();
        let space = Space {
            id: "space-1".to_string(),
            name: "Test".to_string(),
            dir: "/tmp".to_string(),
            grid: GridTemplate::X2x2,
            panes: vec![make_pane("p0", 0), make_pane("p1", 1), make_pane("p2", 2)],
            color: "blue".to_string(),
            created_at: 0,
            last_opened_at: 0,
        };
        state.spaces.push(space);
        state.swap_pane_slots("space-1", 2, 2);
        assert_eq!(state.spaces[0].panes[0].slot_index, 0);
        assert_eq!(state.spaces[0].panes[1].slot_index, 1);
        assert_eq!(state.spaces[0].panes[2].slot_index, 2);
    }

    #[test]
    fn swap_exchanges_slot_indices() {
        let mut state = WorkspaceState::new();
        let space = Space {
            id: "space-1".to_string(),
            name: "Test".to_string(),
            dir: "/tmp".to_string(),
            grid: GridTemplate::X2x2,
            panes: vec![
                make_pane("p0", 0),
                make_pane("p1", 1),
                make_pane("p2", 2),
                make_pane("p3", 3),
            ],
            color: "blue".to_string(),
            created_at: 0,
            last_opened_at: 0,
        };
        state.spaces.push(space);
        state.swap_pane_slots("space-1", 1, 3);
        assert_eq!(state.spaces[0].panes[0].slot_index, 0);
        assert_eq!(state.spaces[0].panes[1].slot_index, 3);
        assert_eq!(state.spaces[0].panes[2].slot_index, 2);
        assert_eq!(state.spaces[0].panes[3].slot_index, 1);
    }

    #[test]
    fn reindex_legacy_workspaces() {
        let mut state = WorkspaceState::new();
        let space = Space {
            id: "space-1".to_string(),
            name: "Test".to_string(),
            dir: "/tmp".to_string(),
            grid: GridTemplate::X2x2,
            panes: vec![make_pane("p0", 0), make_pane("p1", 0), make_pane("p2", 0)],
            color: "blue".to_string(),
            created_at: 0,
            last_opened_at: 0,
        };
        state.spaces.push(space);
        // Simulate the re-index logic from load()
        for space in &mut state.spaces {
            if space.panes.len() > 1 && space.panes.iter().all(|p| p.slot_index == 0) {
                for (i, pane) in space.panes.iter_mut().enumerate() {
                    pane.slot_index = i;
                }
            }
        }
        assert_eq!(state.spaces[0].panes[0].slot_index, 0);
        assert_eq!(state.spaces[0].panes[1].slot_index, 1);
        assert_eq!(state.spaces[0].panes[2].slot_index, 2);
    }

    #[test]
    fn add_pane_sets_slot_index() {
        let mut state = WorkspaceState::new();
        let space = Space {
            id: "space-1".to_string(),
            name: "Test".to_string(),
            dir: "/tmp".to_string(),
            grid: GridTemplate::X1x2,
            panes: vec![make_pane("p0", 0), make_pane("p1", 1)],
            color: "blue".to_string(),
            created_at: 0,
            last_opened_at: 0,
        };
        state.spaces.push(space);
        state.add_pane_to_space("space-1", make_pane("p2", 0));
        assert_eq!(state.spaces[0].panes.len(), 3);
        assert_eq!(state.spaces[0].panes[2].slot_index, 2);
    }

    #[test]
    fn remove_pane_reindexes_remaining() {
        let mut state = WorkspaceState::new();
        let space = Space {
            id: "space-1".to_string(),
            name: "Test".to_string(),
            dir: "/tmp".to_string(),
            grid: GridTemplate::X2x2,
            panes: vec![make_pane("p0", 0), make_pane("p1", 1), make_pane("p2", 2)],
            color: "blue".to_string(),
            created_at: 0,
            last_opened_at: 0,
        };
        state.spaces.push(space);
        state.remove_pane_from_space("space-1", "p1");
        assert_eq!(state.spaces[0].panes.len(), 2);
        assert_eq!(state.spaces[0].panes[0].slot_index, 0);
        assert_eq!(state.spaces[0].panes[1].slot_index, 1);
    }
}
