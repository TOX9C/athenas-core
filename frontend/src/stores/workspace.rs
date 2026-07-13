use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

use crate::tauri_bridge::store_get as kv_get;
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

/// Swap two panes within a space by pane id — *pure* core of
/// [`WorkspaceState::swap_pane_agents`], extracted so the swap logic is
/// unit-testable on the host without the WASM-only `save()` path. Returns
/// `true` if a swap occurred. No-op (returns `false`) if either pane id is
/// missing from the space or the two ids are equal. Slot indices — and
/// therefore each slot's flex-grow size — are unchanged; only the `PaneConfig`
/// values at the two indices trade places (full session migration: `id`,
/// and the PTY session it keys, follows the agent).
pub fn swap_panes_by_id(space: &mut Space, pane_id_a: &str, pane_id_b: &str) -> bool {
    if pane_id_a == pane_id_b {
        return false;
    }
    let ia = space.panes.iter().position(|p| p.id == pane_id_a);
    let ib = space.panes.iter().position(|p| p.id == pane_id_b);
    match (ia, ib) {
        (Some(ia), Some(ib)) => {
            space.panes.swap(ia, ib);
            true
        }
        // one or both ids absent — leave the space untouched
        _ => false,
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

    /// Swap two panes within a space by pane id — full session migration.
    /// The entire `PaneConfig` (including `id`, so the PTY session follows
    /// the agent) trades places between the two slots. Grid slot indices
    /// (and therefore each slot's flex-grow size) are unchanged; only the
    /// values at the two indices swap. Persists via the existing
    /// `update_space`/`save` path. No-op if the space, either pane id is
    /// missing, or the two ids are equal.
    pub fn swap_pane_agents(&mut self, space_id: &str, pane_id_a: &str, pane_id_b: &str) {
        if pane_id_a == pane_id_b {
            return;
        }
        self.update_space(space_id, |space| {
            swap_panes_by_id(space, pane_id_a, pane_id_b);
        });
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
            let _ =
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::UNDEFINED))
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

#[cfg(test)]
mod swap_panes_tests {
    use super::*;
    use crate::types::workspace::{AgentType, PaneConfig, Space};

    fn space_with_panes(ids: &[&str]) -> Space {
        let panes = ids
            .iter()
            .map(|id| PaneConfig {
                id: id.to_string(),
                agent_type: if *id == "shell" {
                    AgentType::Shell
                } else {
                    AgentType::Claude
                },
                label: Some(format!("label-{}", id)),
                ..Default::default()
            })
            .collect();
        Space {
            id: "s1".to_string(),
            name: "S".to_string(),
            dir: "/tmp".to_string(),
            grid: crate::types::workspace::GridTemplate::X1x2,
            panes,
            color: String::new(),
            created_at: 0,
            last_opened_at: 0,
        }
    }

    // NOTE: tests exercise `swap_panes_by_id` (the pure free function), not
    // `swap_pane_agents`. The method is a thin persistence wrapper around
    // `swap_panes_by_id` + `update_space`/`save()`, and `save()` touches
    // js-sys statics that panic on the non-wasm host test target
    // (`cannot access imported statics on non-wasm targets`). Testing the
    // extracted pure core keeps the swap semantics fully covered on the
    // host, mirroring how `grid_for_pane_count` is itself a host-testable
    // free function. The method's wiring (does-it-call-`swap_panes_by_id`)
    // is a compile-time guarantee plus manual smoke verification (plan Task 5).

    #[test]
    fn swaps_two_panes_by_id_full_config_including_id() {
        let mut space = space_with_panes(&["alpha", "beta", "shell"]);
        assert!(swap_panes_by_id(&mut space, "alpha", "beta"));
        // slot 0 now holds beta, slot 1 holds alpha — full PaneConfig swapped
        assert_eq!(space.panes[0].id, "beta");
        assert_eq!(space.panes[0].label.as_deref(), Some("label-beta"));
        assert_eq!(space.panes[1].id, "alpha");
        assert_eq!(space.panes[1].label.as_deref(), Some("label-alpha"));
        // shell untouched at slot 2
        assert_eq!(space.panes[2].id, "shell");
    }

    #[test]
    fn cross_row_swap_swaps_pane_config_only_slots_keep_index() {
        // 2x2: panes indices 0,1 (top row) and 2,3 (bottom row)
        let mut space = space_with_panes(&["a", "b", "c", "d"]);
        assert!(swap_panes_by_id(&mut space, "a", "d"));
        // slot 0 (top-left) now holds what was at slot 3 (bottom-right)
        assert_eq!(space.panes[0].id, "d");
        assert_eq!(space.panes[3].id, "a");
    }

    #[test]
    fn noop_when_ids_equal() {
        let mut space = space_with_panes(&["a", "b"]);
        assert!(!swap_panes_by_id(&mut space, "a", "a"));
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn noop_when_pane_id_missing() {
        let mut space = space_with_panes(&["a", "b"]);
        // first id missing
        assert!(!swap_panes_by_id(&mut space, "ghost", "a"));
        // second id missing
        assert!(!swap_panes_by_id(&mut space, "a", "ghost"));
        // both missing
        assert!(!swap_panes_by_id(&mut space, "x", "y"));
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn preserves_unrelated_panes_and_grid_template() {
        let mut space = space_with_panes(&["a", "b", "c", "shell"]);
        let grid_before = space.grid;
        assert!(swap_panes_by_id(&mut space, "a", "shell"));
        assert_eq!(space.panes.len(), 4);
        assert_eq!(space.grid, grid_before);
    }
}
