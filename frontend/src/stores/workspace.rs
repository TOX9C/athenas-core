use std::cell::RefCell;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

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

/// Single-threaded latest-value queue for workspace persistence.
///
/// `kv_set` is asynchronous, so a generation check before awaiting it cannot
/// prevent an older write from completing after a newer one. Keeping one
/// pending snapshot and draining it from one worker makes writes strictly
/// serial while coalescing bursts of mutations to the newest state.
#[derive(Debug, Default)]
struct WorkspaceSaveQueue {
    pending: Option<String>,
    writing: bool,
}

impl WorkspaceSaveQueue {
    fn enqueue(&mut self, json: String) -> bool {
        self.pending = Some(json);
        if self.writing {
            false
        } else {
            self.writing = true;
            true
        }
    }

    fn take_next(&mut self) -> Option<String> {
        self.pending.take()
    }

    fn finish_write(&mut self) -> bool {
        if self.pending.is_some() {
            true
        } else {
            self.writing = false;
            false
        }
    }
}

thread_local! {
    static SAVE_QUEUE: RefCell<WorkspaceSaveQueue> =
        RefCell::new(WorkspaceSaveQueue::default());
}

fn enqueue_workspace_save(json: String) {
    let should_start_worker = SAVE_QUEUE.with(|queue| queue.borrow_mut().enqueue(json));
    if should_start_worker {
        wasm_bindgen_futures::spawn_local(drain_workspace_saves());
    }
}

async fn drain_workspace_saves() {
    loop {
        let Some(json) = SAVE_QUEUE.with(|queue| queue.borrow_mut().take_next()) else {
            return;
        };

        if let Err(e) = kv_set(WORKSPACES_KEY, &json).await {
            web_sys::console::error_1(&format!("[WorkspaceState] store_set error: {:?}", e).into());
        }

        let has_pending_write = SAVE_QUEUE.with(|queue| queue.borrow_mut().finish_write());
        if !has_pending_write {
            return;
        }
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

    pub fn update_space(&mut self, id: &str, f: impl FnOnce(&mut Space)) -> bool {
        let Some(space) = self.spaces.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        let before = space.clone();
        f(space);
        if *space == before {
            return false;
        }
        self.save();
        true
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
    /// Saves are coalesced and drained serially so overlapping async writes
    /// never result in a stale state overwriting a newer one.
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

        enqueue_workspace_save(json);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_space_does_not_report_or_save_when_nothing_changes() {
        let space = Space {
            id: "space-1".to_string(),
            ..Space::default()
        };
        let mut state = WorkspaceState {
            spaces: vec![space],
            active_space_id: Some("space-1".to_string()),
        };

        assert!(!state.update_space("space-1", |_| {}));
        assert!(!state.update_space("missing", |_| {}));
    }

    #[test]
    fn repeated_resume_capture_is_idempotent() {
        let pane = PaneConfig {
            id: "pane-1".to_string(),
            resume_id: Some("2026-08-17T21-35-03.500Z".to_string()),
            resume_cmd: Some("freebuff --continue 2026-08-17T21-35-03.500Z".to_string()),
            resume_dismissed: Some(false),
            ..PaneConfig::default()
        };
        let space = Space {
            id: "space-1".to_string(),
            panes: vec![pane],
            ..Space::default()
        };
        let mut state = WorkspaceState {
            spaces: vec![space],
            active_space_id: Some("space-1".to_string()),
        };

        assert!(!state.update_space("space-1", |space| {
            let pane = space.panes.first_mut().expect("test pane");
            pane.resume_id = Some("2026-08-17T21-35-03.500Z".to_string());
            pane.resume_cmd = Some("freebuff --continue 2026-08-17T21-35-03.500Z".to_string());
            pane.resume_dismissed = Some(false);
        }));
    }

    #[test]
    fn save_queue_serializes_writes_and_keeps_only_the_latest_pending_state() {
        let mut queue = WorkspaceSaveQueue::default();

        assert!(queue.enqueue("old".to_string()));
        assert_eq!(queue.take_next().as_deref(), Some("old"));

        assert!(!queue.enqueue("newer".to_string()));
        assert!(!queue.enqueue("newest".to_string()));
        assert!(queue.finish_write());
        assert_eq!(queue.take_next().as_deref(), Some("newest"));
        assert!(!queue.finish_write());
    }
}
