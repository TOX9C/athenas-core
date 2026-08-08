//! Pure agent-output contracts and bounded-buffer policy.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single line of agent output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputLine {
    pub pane_id: String,
    pub line_num: usize,
    pub timestamp: i64,
    pub text: String,
    /// Precomputed stderr heuristic. Set once at line arrival; never
    /// re-evaluated during render. See [`is_stderr_like`].
    pub is_stderr: bool,
}

/// Heuristic: text containing error/warn/fail/exception keywords looks like
/// stderr output. Called once per line at construction time (in the IPC
/// listener), never per render.
///
/// Returns the same answer as the legacy render-time heuristic. Allocates a
/// single lowercased copy of `text` per call — at most once per incoming line.
pub fn is_stderr_like(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("error")
        || lower.contains("warn")
        || lower.contains("fail")
        || lower.contains("exception")
}

/// Metadata about an agent's output stream.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentOutputInfo {
    pub pane_id: String,
    pub agent_type: String,
    pub line_count: usize,
    pub created_at: i64,
    pub last_activity_at: i64,
}

/// State of a subscription to a pane's output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubscriptionState {
    pub subscription_id: Option<String>,
    pub pane_id: Option<String>,
    pub active: bool,
}

/// Maximum lines retained per output buffer.
pub(super) const MAX_LINES_PER_BUFFER: usize = 5000;
/// Maximum characters retained per output line (prevents DoS from huge single lines).
pub(super) const MAX_TEXT_LENGTH: usize = 10000;
/// Maximum number of tracked output panes.
pub(super) const MAX_PANE_COUNT: usize = 100;
/// Target pane count after garbage collection.
pub(super) const PANE_GC_TARGET: usize = 80;
/// Threshold for idle-pane GC (milliseconds). A pane whose buffer has not been
/// touched for this duration is eligible for eviction.
pub(super) const PANE_GC_IDLE_THRESHOLD_MS: u64 = 30 * 60 * 1000;
/// Recommended sweep interval for periodic GC (milliseconds).
pub const PANE_GC_INTERVAL_MS: u64 = 5 * 60 * 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_classifier_matches_error_words() {
        assert!(is_stderr_like("ERROR: failed to connect"));
        assert!(is_stderr_like("warning: deprecated option"));
        assert!(is_stderr_like("Unhandled exception"));
        assert!(!is_stderr_like("build completed successfully"));
    }

    #[test]
    fn output_contract_defaults_are_empty() {
        assert_eq!(
            OutputLine::default(),
            OutputLine {
                pane_id: String::new(),
                line_num: 0,
                timestamp: 0,
                text: String::new(),
                is_stderr: false,
            }
        );
        assert_eq!(AgentOutputInfo::default().line_count, 0);
        assert_eq!(SubscriptionState::default().active, false);
    }
}
