use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Status of an individual agent.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AgentRunStatus {
    #[default]
    Idle,
    Thinking,
    Working,
    WaitingForInput,
    Completed,
    Error,
    Cancelled,
    Disconnected,
}

/// Progress information for an agent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentProgress {
    pub current: usize,
    pub total: usize,
    pub label: String,
}

/// Status record for a single agent (keyed by pane id).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentStatus {
    pub pane_id: String,
    pub status: AgentRunStatus,
    pub message: Option<String>,
    pub progress: Option<AgentProgress>,
    pub last_updated_at: i64,
}

/// Partial update descriptor for an agent status entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentStatusUpdate {
    pub status: Option<AgentRunStatus>,
    pub message: Option<String>,
    pub progress: Option<AgentProgress>,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global agent status tracking state.
#[derive(Clone, PartialEq, Default)]
pub struct AgentStatusState {
    pub statuses: Vec<(String, AgentStatus)>,
}

impl AgentStatusState {
    pub fn new() -> Self {
        Self {
            statuses: Vec::new(),
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    /// Update (or insert) the status for a pane.
    pub fn update_status(
        &mut self,
        pane_id: impl Into<String>,
        update: AgentStatusUpdate,
        now: i64,
    ) {
        let key = pane_id.into();
        if let Some(entry) = self.statuses.iter_mut().find(|(id, _)| id == &key) {
            entry.1.last_updated_at = now;
            if let Some(status) = update.status {
                entry.1.status = status;
            }
            if let Some(message) = update.message {
                entry.1.message = Some(message);
            }
            if let Some(progress) = update.progress {
                entry.1.progress = Some(progress);
            }
        } else {
            self.statuses.push((
                key.clone(),
                AgentStatus {
                    pane_id: key,
                    status: update.status.unwrap_or_default(),
                    message: update.message,
                    progress: update.progress,
                    last_updated_at: now,
                },
            ));
        }
    }

    /// Remove the status entry for a pane.
    pub fn remove_status(&mut self, pane_id: &str) {
        self.statuses.retain(|(id, _)| id != pane_id);
    }

    // -- Event handlers for Tauri push events -------------------------------

    /// Maximum number of agent status entries to retain.
    const MAX_STATUSES: usize = 500;
    /// Target count after LRU eviction.
    const STATUS_GC_TARGET: usize = 400;

    /// Handle an agent connected event.
    /// If the pane already exists (e.g. reconnect), reset it to Idle.
    pub fn connect_agent(&mut self, pane_id: String, now: i64) {
        if let Some(entry) = self.statuses.iter_mut().find(|(id, _)| id == &pane_id) {
            entry.1.status = AgentRunStatus::Idle;
            entry.1.message = Some("Connected".to_string());
            entry.1.progress = None;
            entry.1.last_updated_at = now;
        } else {
            self.statuses.push((
                pane_id.clone(),
                AgentStatus {
                    pane_id,
                    status: AgentRunStatus::Idle,
                    message: Some("Connected".to_string()),
                    progress: None,
                    last_updated_at: now,
                },
            ));
        }
        self.maybe_gc_statuses();
    }

    /// Handle an agent disconnected event — removes the entry entirely
    /// to prevent unbounded growth.
    pub fn disconnect_agent(&mut self, pane_id: &str, _now: i64) {
        self.statuses.retain(|(id, _)| id != pane_id);
    }

    /// Evict oldest statuses when we exceed the hard cap.
    fn maybe_gc_statuses(&mut self) {
        if self.statuses.len() <= Self::MAX_STATUSES {
            return;
        }
        // Sort by last_updated_at (oldest first) and trim to target.
        self.statuses.sort_by_key(|(_, s)| s.last_updated_at);
        let to_remove = self.statuses.len().saturating_sub(Self::STATUS_GC_TARGET);
        self.statuses.drain(0..to_remove);
    }

    /// Handle an input requested event — add a notification-worthy status.
    pub fn request_input(&mut self, pane_id: String, message: String, now: i64) {
        if let Some(entry) = self.statuses.iter_mut().find(|(id, _)| id == &pane_id) {
            entry.1.status = AgentRunStatus::WaitingForInput;
            entry.1.message = Some(message);
            entry.1.last_updated_at = now;
        } else {
            self.statuses.push((
                pane_id.clone(),
                AgentStatus {
                    pane_id,
                    status: AgentRunStatus::WaitingForInput,
                    message: Some(message),
                    progress: None,
                    last_updated_at: now,
                },
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the agent status signal from the Dioxus context.
pub fn use_agent_status_store() -> Signal<AgentStatusState> {
    use_context::<Signal<AgentStatusState>>()
}

/// Initialize the agent status store as a context provider.
pub fn provide_agent_status_store() {
    use_context_provider(|| Signal::new(AgentStatusState::new()));
}
