use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[path = "agent_status_model.rs"]
mod agent_status_model;

pub use agent_status_model::{AgentProgress, AgentRunStatus, AgentStatus, AgentStatusUpdate};

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Per-pane agent status registry, mirroring the `TerminalRegistry` /
/// `AgentOutputStore` pattern.
///
/// Previously a single `Signal<AgentStatusState>` held every pane's status;
/// one backend `agent:status` heartbeat for pane A re-validated every
/// subscriber anywhere in the store (status bars of all panes, pane pills,
/// inbox badge) at heartbeat rate. Now each pane's status lives in a
/// lazily-created `Signal<AgentStatus>` keyed in a `HashMap`, so a heartbeat
/// write for pane A invalidates only pane A's subscribers, and the
/// stale-generation check is an O(1) map lookup instead of an O(n) scan.
///
/// Membership (`members`) is a small separate signal that changes only on
/// insert/remove, so list views re-render on membership change while
/// value-level updates stay per-pane.
#[derive(Clone)]
pub struct AgentStatusRegistry {
    statuses: Rc<RefCell<HashMap<String, Signal<AgentStatus>>>>,
    /// Pane ids in insert order. Written only on insert/remove.
    members: Signal<Vec<String>>,
}

impl Default for AgentStatusRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentStatusRegistry {
    pub fn new() -> Self {
        Self {
            statuses: Rc::new(RefCell::new(HashMap::new())),
            members: Signal::new(Vec::new()),
        }
    }

    // -- Signal accessors ---------------------------------------------------

    /// Reactive membership list (pane ids, insert order). Changes only on
    /// register/remove — subscribe here for list views.
    pub fn members_signal(&self) -> Signal<Vec<String>> {
        self.members
    }

    /// The reactive per-pane status, or `None` if no status exists for `id`.
    pub fn status_signal(&self, pane_id: &str) -> Option<Signal<AgentStatus>> {
        self.statuses.borrow().get(pane_id).cloned()
    }

    /// Lazily create the inner `Signal<AgentStatus>` for `pane_id` if absent.
    fn ensure_status(&self, pane_id: &str) -> Signal<AgentStatus> {
        if let Some(signal) = self.statuses.borrow().get(pane_id).cloned() {
            return signal;
        }
        let signal = Signal::new_in_scope(
            AgentStatus {
                pane_id: pane_id.to_string(),
                ..Default::default()
            },
            ScopeId::APP,
        );
        self.statuses
            .borrow_mut()
            .insert(pane_id.to_string(), signal);
        self.members.write_unchecked().push(pane_id.to_string());
        signal
    }

    /// Read a pane's status field without subscribing (peek).
    pub fn peek_status<T>(
        &self,
        pane_id: &str,
        f: impl FnOnce(&AgentStatus) -> T,
    ) -> Option<T> {
        let signal = self.statuses.borrow().get(pane_id).cloned()?;
        let guard = signal.peek();
        Some(f(&guard))
    }

    /// Snapshot of all statuses (pane id, status) in membership order.
    ///
    /// Reads each pane signal reactively: the caller subscribes to membership
    /// and to every pane's value, so list views update on both structural
    /// and per-pane changes.
    pub fn snapshot(&self) -> Vec<(String, AgentStatus)> {
        self.members
            .read()
            .iter()
            .filter_map(|id| {
                self.statuses
                    .borrow()
                    .get(id)
                    .map(|s| (id.clone(), s.read().clone()))
            })
            .collect()
    }

    /// All live × un-dismissed "waiting for input" panes, oldest first.
    /// Non-reactive peek — for one-shot callers; reactive consumers should
    /// iterate `snapshot()` instead.
    pub fn waiting_panes(&self) -> Vec<String> {
        self.members
            .peek()
            .iter()
            .filter(|id| {
                self.peek_status(id, |s| {
                    matches!(s.status, AgentRunStatus::WaitingForInput) && !s.dismissed
                })
                .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    // -- Mutators ------------------------------------------------------------

    /// Update (or insert) the status for a pane. Touches ONLY that pane's
    /// signal on value writes; membership signal only on insert.
    pub fn update_status(
        &self,
        pane_id: impl Into<String>,
        update: AgentStatusUpdate,
        now: i64,
    ) {
        let key = pane_id.into();
        if let Some(signal) = self.status_signal(&key) {
            let mut entry = signal.write_unchecked();
            // A late event from an older PTY must not overwrite the status of
            // a newer PTY that reused the same pane id. Legacy updates without
            // a generation remain accepted for plugin/backward compatibility.
            if let (Some(current), Some(incoming)) = (entry.generation, update.generation) {
                if current != incoming {
                    return;
                }
            }
            entry.last_updated_at = now;
            if let Some(generation) = update.generation {
                entry.generation = Some(generation);
            }
            if let Some(status) = update.status {
                entry.status = status;
            }
            if let Some(message) = update.message {
                entry.message = Some(message);
            }
            if let Some(progress) = update.progress {
                entry.progress = Some(progress);
            }
        } else {
            let signal = self.ensure_status(&key);
            let mut entry = signal.write_unchecked();
            entry.generation = update.generation;
            entry.status = update.status.unwrap_or_default();
            entry.message = update.message;
            entry.progress = update.progress;
            entry.last_updated_at = now;
        }
    }

    /// Remove the status entry for a pane.
    pub fn remove_status(&self, pane_id: &str) {
        if self.statuses.borrow_mut().remove(pane_id).is_some() {
            self.members.write_unchecked().retain(|id| id != pane_id);
        }
    }

    pub fn dismiss_waiting(&self, pane_id: &str) {
        if let Some(mut entry) = self
            .status_signal(pane_id)
            .map(|s| s.write_unchecked())
        {
            entry.dismissed = true;
            entry.status = AgentRunStatus::Idle;
        }
    }

    pub fn dismiss_all_waiting(&self) {
        let ids = self.members.peek().clone();
        for id in ids {
            if let Some(signal) = self.status_signal(&id) {
                let mut entry = signal.write_unchecked();
                if entry.status == AgentRunStatus::WaitingForInput {
                    entry.dismissed = true;
                    entry.status = AgentRunStatus::Idle;
                }
            }
        }
    }

    // -- Event handlers for Tauri push events --------------------------------

    /// Handle an agent connected event.
    pub fn connect_agent(&self, pane_id: String, now: i64) {
        if self.status_signal(&pane_id).is_none() {
            let signal = self.ensure_status(&pane_id);
            let mut entry = signal.write_unchecked();
            entry.status = AgentRunStatus::Idle;
            entry.message = Some("Connected".to_string());
            entry.last_updated_at = now;
        }
    }

    /// Handle an agent disconnected event.
    pub fn disconnect_agent(&self, pane_id: &str, now: i64) {
        if let Some(signal) = self.status_signal(pane_id) {
            let mut entry = signal.write_unchecked();
            entry.status = AgentRunStatus::Disconnected;
            entry.message = Some("Disconnected".to_string());
            entry.last_updated_at = now;
        }
    }

    /// Handle an input requested event — add a notification-worthy status.
    pub fn request_input(&self, pane_id: String, message: String, now: i64) {
        let signal = self.ensure_status(&pane_id);
        let mut entry = signal.write_unchecked();
        entry.status = AgentRunStatus::WaitingForInput;
        entry.message = Some(message);
        entry.last_updated_at = now;
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the agent status registry from the Dioxus context. Cloning the
/// returned registry is a cheap Rc bump; reads subscribe per pane.
pub fn use_agent_status_store() -> AgentStatusRegistry {
    use_context::<AgentStatusRegistry>()
}

/// Click-to-acknowledge: clear the pending-attention (waiting-for-input) flag
/// for a pane so its ring/dot stops rendering until the next input event.
pub fn acknowledge_pane_attention(store: AgentStatusRegistry, pane_id: &str) {
    store.dismiss_waiting(pane_id);
}

/// Initialize the agent status store as a context provider.
pub fn provide_agent_status_store() {
    use_context_provider(AgentStatusRegistry::new);
}
