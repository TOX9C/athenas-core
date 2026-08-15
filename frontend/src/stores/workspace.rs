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

#[path = "workspace_helpers.rs"]
mod workspace_helpers;

pub use workspace_helpers::{grid_for_pane_count, swap_panes_by_id};

/// Key used in KeyValueStore for workspace persistence.
const WORKSPACES_KEY: &str = "workspaces";

/// Monotonic generation counter for coalescing concurrent saves attempts.
static SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);

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
                web_sys::console::log_1(
                    &format!(
                        "[resume-debug] workspace store_get succeeded bytes={}",
                        json.len()
                    )
                    .into(),
                );
                if json.trim().is_empty() {
                    web_sys::console::warn_1(&"[resume-debug] workspace store is empty".into());
                    return Self::new();
                }
                match serde_json::from_str(&json) {
                    Ok(state) => state,
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("[resume-debug] workspace deserialize failed: {e}").into(),
                        );
                        Self::new()
                    }
                }
            }
            Err(e) => {
                // Key absent on first run — not an error.
                web_sys::console::warn_1(
                    &format!("[resume-debug] workspace store_get failed/absent: {e:?}").into(),
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
