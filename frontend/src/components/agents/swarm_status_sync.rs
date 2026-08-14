//! Rate-limits persistence of live swarm status updates.
//!
//! Backend status events can arrive much faster than the durable swarm state
//! needs to change. This keeps meaningful transitions immediate while limiting
//! repeated heartbeat/action writes to at most one per interval per pane.

use std::collections::HashMap;

const MIN_SYNC_INTERVAL_MS: i64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SwarmStatusUpdate {
    pub(super) dir: String,
    pub(super) agent_id: String,
    pub(super) status: &'static str,
    pub(super) last_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LastSync {
    update: SwarmStatusUpdate,
    sent_at: i64,
}

#[derive(Debug, Default)]
pub(super) struct SwarmStatusSync {
    last_by_pane: HashMap<String, LastSync>,
}

/// Guards the status bus against late events from a retired PTY generation.
#[derive(Debug, Default)]
pub(super) struct PaneGenerationGuard {
    retired: HashMap<String, Option<u64>>,
}

impl PaneGenerationGuard {
    pub(super) fn retire(&mut self, pane_id: &str, generation: Option<u64>) {
        self.retired.insert(pane_id.to_string(), generation);
    }

    pub(super) fn accepts(&self, pane_id: &str, incoming: Option<u64>) -> bool {
        match (self.retired.get(pane_id), incoming) {
            (None, _) => true,
            (Some(Some(retired)), Some(incoming)) => incoming > *retired,
            // A generation-less event cannot prove that it belongs to a new
            // PTY, so hold it until an explicit connection/new generation.
            (Some(_), None) => false,
            // A generation-bearing event is enough to establish a new lease
            // after a legacy exit/disconnect tombstone.
            (Some(None), Some(_)) => true,
        }
    }

    pub(super) fn reopen(&mut self, pane_id: &str) {
        self.retired.remove(pane_id);
    }
}

impl SwarmStatusSync {
    /// Return whether an update should be persisted now and record it when it
    /// is allowed. Status/identity transitions are immediate; repeated
    /// updates for the same pane are limited to one per second.
    pub(super) fn should_send(
        &mut self,
        pane_id: &str,
        update: SwarmStatusUpdate,
        now: i64,
    ) -> bool {
        let should_send = match self.last_by_pane.get(pane_id) {
            None => true,
            Some(previous) => {
                let identity_or_status_changed = previous.update.dir != update.dir
                    || previous.update.agent_id != update.agent_id
                    || previous.update.status != update.status;
                let action_changed = previous.update.last_action != update.last_action;
                let interval_elapsed = now.saturating_sub(previous.sent_at) >= MIN_SYNC_INTERVAL_MS;
                identity_or_status_changed || action_changed || interval_elapsed
            }
        };

        if should_send {
            self.last_by_pane.insert(
                pane_id.to_string(),
                LastSync {
                    update,
                    sent_at: now,
                },
            );
        }

        should_send
    }

    pub(super) fn remove(&mut self, pane_id: &str) {
        self.last_by_pane.remove(pane_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(status: &'static str, action: &str) -> SwarmStatusUpdate {
        SwarmStatusUpdate {
            dir: "/workspace".to_string(),
            agent_id: "agent-1".to_string(),
            status,
            last_action: Some(action.to_string()),
        }
    }

    #[test]
    fn retired_generation_rejects_late_events_and_allows_new_generation() {
        let mut guard = PaneGenerationGuard::default();
        guard.retire("pane-1", Some(4));
        assert!(!guard.accepts("pane-1", Some(4)));
        assert!(!guard.accepts("pane-1", Some(3)));
        assert!(guard.accepts("pane-1", Some(5)));
        guard.reopen("pane-1");
        assert!(guard.accepts("pane-1", None));
    }

    #[test]
    fn legacy_tombstone_rejects_generationless_events() {
        let mut guard = PaneGenerationGuard::default();
        guard.retire("pane-1", None);
        assert!(!guard.accepts("pane-1", None));
        assert!(guard.accepts("pane-1", Some(1)));
    }

    #[test]
    fn sends_first_update() {
        let mut sync = SwarmStatusSync::default();
        assert!(sync.should_send("pane-1", update("writing", "started"), 1_000));
    }

    #[test]
    fn suppresses_duplicate_update_within_interval() {
        let mut sync = SwarmStatusSync::default();
        assert!(sync.should_send("pane-1", update("writing", "same"), 1_000));
        assert!(!sync.should_send("pane-1", update("writing", "same"), 1_999));
    }

    #[test]
    fn allows_same_status_after_interval() {
        let mut sync = SwarmStatusSync::default();
        assert!(sync.should_send("pane-1", update("writing", "same"), 1_000));
        assert!(sync.should_send("pane-1", update("writing", "same"), 2_000));
    }

    #[test]
    fn sends_action_change_immediately() {
        let mut sync = SwarmStatusSync::default();
        assert!(sync.should_send("pane-1", update("writing", "started"), 1_000));
        assert!(sync.should_send("pane-1", update("writing", "finished step"), 1_001));
    }

    #[test]
    fn sends_status_transition_immediately() {
        let mut sync = SwarmStatusSync::default();
        assert!(sync.should_send("pane-1", update("writing", "started"), 1_000));
        assert!(sync.should_send("pane-1", update("done", "finished"), 1_001));
    }

    #[test]
    fn removing_pane_resets_rate_limit() {
        let mut sync = SwarmStatusSync::default();
        assert!(sync.should_send("pane-1", update("writing", "same"), 1_000));
        sync.remove("pane-1");
        assert!(sync.should_send("pane-1", update("writing", "same"), 1_001));
    }
}
