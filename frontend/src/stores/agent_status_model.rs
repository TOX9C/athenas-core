//! Pure agent status contracts shared by the store and status UI.

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
    /// PTY generation owning this transient status. Used to ignore a late
    /// exit event from an older process after a pane id is reused.
    pub generation: Option<u64>,
    pub status: AgentRunStatus,
    pub message: Option<String>,
    pub progress: Option<AgentProgress>,
    pub last_updated_at: i64,
}

/// Partial update descriptor for an agent status entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentStatusUpdate {
    pub generation: Option<u64>,
    pub status: Option<AgentRunStatus>,
    pub message: Option<String>,
    pub progress: Option<AgentProgress>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_idle_and_empty() {
        assert_eq!(AgentRunStatus::default(), AgentRunStatus::Idle);
        assert_eq!(
            AgentProgress::default(),
            AgentProgress {
                current: 0,
                total: 0,
                label: String::new()
            }
        );
        assert_eq!(
            AgentStatusUpdate::default(),
            AgentStatusUpdate {
                generation: None,
                status: None,
                message: None,
                progress: None
            }
        );
    }

    #[test]
    fn status_contract_holds_expected_fields() {
        let status = AgentStatus {
            pane_id: "pane-1".to_string(),
            generation: Some(1),
            status: AgentRunStatus::Working,
            message: Some("Running".to_string()),
            progress: Some(AgentProgress {
                current: 2,
                total: 4,
                label: "step".to_string(),
            }),
            last_updated_at: 42,
        };
        assert_eq!(status.pane_id, "pane-1");
        assert_eq!(status.status, AgentRunStatus::Working);
        assert_eq!(status.progress.as_ref().map(|p| p.current), Some(2));
        assert_eq!(status.last_updated_at, 42);
    }

    #[test]
    fn terminal_exit_cleanup_removes_transient_status() {
        let mut state = crate::stores::agent_status::AgentStatusState::new();
        state.update_status(
            "pane-1",
            AgentStatusUpdate {
                generation: Some(1),
                status: Some(AgentRunStatus::Working),
                ..Default::default()
            },
            1,
        );
        state.remove_status("pane-1");
        assert!(state.statuses.is_empty());
    }

    #[test]
    fn stale_generation_update_cannot_overwrite_reused_pane() {
        let mut state = crate::stores::agent_status::AgentStatusState::new();
        state.update_status(
            "pane-1",
            AgentStatusUpdate {
                generation: Some(2),
                status: Some(AgentRunStatus::Working),
                ..Default::default()
            },
            2,
        );
        state.update_status(
            "pane-1",
            AgentStatusUpdate {
                generation: Some(1),
                status: Some(AgentRunStatus::Completed),
                ..Default::default()
            },
            3,
        );
        let status = &state.statuses[0].1;
        assert_eq!(status.generation, Some(2));
        assert_eq!(status.status, AgentRunStatus::Working);
        assert_eq!(status.last_updated_at, 2);
    }
}
