//! Pure per-space agent-count helpers shared by the workspace tab bar and the
//! sidebar workspace list.
//!
//! The workspace tab shows three badges per space, left → right:
//! `[working] [total] [attention]`. `total` is the legacy "how many agents are
//! in this space" count; `working` sits to its LEFT (the user's explicit ask:
//! "to the left of that four, the amount of agents that are working"); and
//! `attention` marks agents that finished work, are waiting for input, or
//! errored.

use crate::stores::agent_status::{AgentRunStatus, AgentStatus};
use crate::types::workspace::{AgentType, PaneConfig};

/// Aggregate live agent counts for a space's panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpaceCounts {
    /// Panes that host an agent: configured agent panes plus any pane whose
    /// status entry shows a live (non-disconnected) agent session.
    pub total: usize,
    /// Panes whose agent is actively working (`Working` | `Thinking`).
    pub working: usize,
    /// Panes that need the user's attention: finished work (`Completed`),
    /// waiting for input, or errored.
    pub attention: usize,
}

impl SpaceCounts {
    /// True when the space has at least one agent that needs attention.
    pub fn has_attention(&self) -> bool {
        self.attention > 0
    }
}

/// True when the pane *type* is an agent (everything except a plain shell).
pub fn is_agent_pane_type(at: &AgentType) -> bool {
    !matches!(at, AgentType::Shell)
}

/// Count working/total/attention for a space's panes from the agent-status map.
///
/// `statuses` is the `AgentStatusState.statuses` slice (pane id → status).
/// A pane counts toward `total` when its configured type is an agent, or when
/// a status entry records a live (non-disconnected) agent session inside it
/// (e.g. `claude` typed manually into a Shell pane).
///
/// An `Idle` status entry is NOT proof of an agent: the backend tracker emits
/// `agent:status` (with `Idle`) for any pane whose raw foreground label
/// changes — including plain Shell panes (so their pills can show `vim`, etc.)
/// — so a Shell pane with an `Idle` entry must still count as shell-only.
///
/// Known edge: a real agent typed manually into a Shell pane that goes silent
/// between turns also receives an `Idle` status, so it drops from `total`
/// until it emits again. The backend payload carries `fgProcess` (which stays
/// `"claude"` for a silent-but-foreground agent vs empty for a shell) that
/// could disambiguate this if ever needed; accepted as a minor cosmetic edge.
pub fn count_space_agents(panes: &[PaneConfig], statuses: &[(String, AgentStatus)]) -> SpaceCounts {
    let mut counts = SpaceCounts::default();
    for pane in panes {
        let status = statuses
            .iter()
            .find(|(id, _)| id == &pane.id)
            .map(|(_, s)| &s.status);
        let agent_present = match status {
            Some(AgentRunStatus::Disconnected) => false,
            // Idle means "no agent detected" for a shell; only an agent-typed
            // pane counts as an agent while idle (e.g. quiet between turns).
            Some(AgentRunStatus::Idle) => is_agent_pane_type(&pane.agent_type),
            Some(_) => true,
            None => is_agent_pane_type(&pane.agent_type),
        };
        if agent_present {
            counts.total += 1;
        }
        match status {
            Some(AgentRunStatus::Working) | Some(AgentRunStatus::Thinking) => counts.working += 1,
            Some(AgentRunStatus::WaitingForInput)
            | Some(AgentRunStatus::Error)
            | Some(AgentRunStatus::Completed) => counts.attention += 1,
            _ => {}
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: &str, at: AgentType) -> PaneConfig {
        PaneConfig {
            id: id.to_string(),
            agent_type: at,
            ..Default::default()
        }
    }

    fn status(pane_id: &str, status: AgentRunStatus) -> (String, AgentStatus) {
        (
            pane_id.to_string(),
            AgentStatus {
                pane_id: pane_id.to_string(),
                status,
                ..Default::default()
            },
        )
    }

    #[test]
    fn counts_agent_panes_with_no_status_as_total() {
        let panes = [
            pane("p1", AgentType::Claude),
            pane("p2", AgentType::Shell),
            pane("p3", AgentType::Qwen),
        ];
        let c = count_space_agents(&panes, &[]);
        assert_eq!(c.total, 2);
        assert_eq!(c.working, 0);
        assert_eq!(c.attention, 0);
    }

    #[test]
    fn working_counts_working_and_thinking() {
        let panes = [
            pane("p1", AgentType::Claude),
            pane("p2", AgentType::Codex),
            pane("p3", AgentType::Gemini),
        ];
        let statuses = [
            status("p1", AgentRunStatus::Working),
            status("p2", AgentRunStatus::Thinking),
            status("p3", AgentRunStatus::Idle),
        ];
        let c = count_space_agents(&panes, &statuses);
        assert_eq!(c.total, 3);
        assert_eq!(c.working, 2);
        assert_eq!(c.attention, 0);
    }

    #[test]
    fn attention_counts_completed_input_and_error() {
        let panes = [
            pane("p1", AgentType::Claude),
            pane("p2", AgentType::Codex),
            pane("p3", AgentType::Gemini),
            pane("p4", AgentType::Aider),
        ];
        let statuses = [
            status("p1", AgentRunStatus::Completed),
            status("p2", AgentRunStatus::WaitingForInput),
            status("p3", AgentRunStatus::Error),
            status("p4", AgentRunStatus::Working),
        ];
        let c = count_space_agents(&panes, &statuses);
        assert_eq!(c.total, 4);
        assert_eq!(c.working, 1);
        assert_eq!(c.attention, 3);
        assert!(c.has_attention());
    }

    #[test]
    fn disconnected_agent_drops_from_total() {
        let panes = [pane("p1", AgentType::Claude), pane("p2", AgentType::Shell)];
        // p2 is a Shell pane that had an agent (tracker entry) but the PTY
        // exited → Disconnected → no agent present anymore.
        let statuses = [status("p2", AgentRunStatus::Disconnected)];
        let c = count_space_agents(&panes, &statuses);
        assert_eq!(c.total, 1);
        assert_eq!(c.working, 0);
        assert_eq!(c.attention, 0);
    }

    #[test]
    fn agent_detected_inside_shell_pane_counts() {
        // `claude` typed into a plain Shell pane → the tracker emits a live
        // status entry → the pane counts toward total even though its type is
        // Shell.
        let panes = [pane("p1", AgentType::Shell)];
        let statuses = [status("p1", AgentRunStatus::Working)];
        let c = count_space_agents(&panes, &statuses);
        assert_eq!(c.total, 1);
        assert_eq!(c.working, 1);
        assert_eq!(c.attention, 0);
    }

    #[test]
    fn idle_status_entry_on_shell_pane_is_not_an_agent() {
        // The tracker emits `agent:status` (Idle) for shell panes whenever
        // their raw foreground label changes (pills need fgProcess). An Idle
        // entry must NOT make a Shell pane count toward total — shells show
        // zero badges.
        let panes = [
            pane("p1", AgentType::Shell),
            pane("p2", AgentType::Shell),
            pane("p3", AgentType::Shell),
        ];
        let statuses = [
            status("p1", AgentRunStatus::Idle),
            status("p2", AgentRunStatus::Idle),
            status("p3", AgentRunStatus::Idle),
        ];
        let c = count_space_agents(&panes, &statuses);
        assert_eq!(c, SpaceCounts::default());
    }

    #[test]
    fn agent_pane_stays_counted_while_idle() {
        // A Claude pane between turns is Idle but is still an agent — its
        // configured type keeps it in `total` even when the tracker emits Idle.
        let panes = [pane("p1", AgentType::Claude)];
        let statuses = [status("p1", AgentRunStatus::Idle)];
        let c = count_space_agents(&panes, &statuses);
        assert_eq!(c.total, 1);
        assert_eq!(c.working, 0);
        assert_eq!(c.attention, 0);
    }

    #[test]
    fn empty_space_is_zero() {
        let c = count_space_agents(&[], &[]);
        assert_eq!(c, SpaceCounts::default());
    }
}
