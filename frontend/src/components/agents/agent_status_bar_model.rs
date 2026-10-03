//! Pure agent status-bar model and formatting helpers.

use crate::stores::agent_status::{AgentRunStatus, AgentStatus};

/// Status info for an agent pane.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentPaneStatus {
    pub pane_id: String,
    pub status: String,
    pub agent_type: String,
    pub message: String,
    pub progress: Option<ProgressInfo>,
    pub last_updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProgressInfo {
    pub current: usize,
    pub total: usize,
    pub label: Option<String>,
}

/// Get the (short, readable) status labels and color for a given status string.
pub(super) fn status_label(status: &str) -> (&'static str, &'static str, &'static str) {
    match status {
        "thinking" | "working" => ("RUN", "Running", "var(--accent)"),
        "waiting_for_input" => ("WAIT", "Waiting", "var(--warning)"),
        "completed" => ("OK", "Done", "var(--success)"),
        "error" => ("ERR", "Error", "var(--error)"),
        "cancelled" => ("CX", "Cancelled", "var(--textDim)"),
        "disconnected" => ("OFF", "Offline", "var(--textDim)"),
        "idle" => ("IDLE", "Idle", "var(--textDim)"),
        _ => ("???", "Unknown", "var(--textDim)"),
    }
}

pub(super) fn time_ago(ts: i64) -> String {
    let now_ms = js_sys::Date::now() as i64;
    let diff_secs = ((now_ms - ts) / 1000).max(0) as u64;
    if diff_secs < 5 {
        return "now".to_string();
    }
    if diff_secs < 60 {
        return format!("{}s", diff_secs);
    }
    if diff_secs < 3600 {
        return format!("{}m", diff_secs / 60);
    }
    format!("{}h", diff_secs / 3600)
}

/// Dot color for an inspector status row, keyed by status string.
pub fn status_dot_color(status: &str) -> &'static str {
    match status {
        "idle" => "var(--textDim)",
        "thinking" => "var(--accent)",
        "working" => "var(--accent)",
        "waiting_for_input" => "var(--warning)",
        "completed" => "var(--success)",
        "error" => "var(--error)",
        "cancelled" => "var(--error)",
        "disconnected" => "var(--textDim)",
        _ => "var(--textDim)",
    }
}

/// Convert store AgentStatus to the component-level AgentPaneStatus.
pub fn to_pane_status(agent_status: &AgentStatus) -> AgentPaneStatus {
    AgentPaneStatus {
        pane_id: agent_status.pane_id.clone(),
        status: match agent_status.status {
            AgentRunStatus::Idle => "idle".to_string(),
            AgentRunStatus::Thinking => "thinking".to_string(),
            AgentRunStatus::Working => "working".to_string(),
            AgentRunStatus::WaitingForInput => "waiting_for_input".to_string(),
            AgentRunStatus::Completed => "completed".to_string(),
            AgentRunStatus::Error => "error".to_string(),
            AgentRunStatus::Cancelled => "cancelled".to_string(),
            AgentRunStatus::Disconnected => "disconnected".to_string(),
        },
        agent_type: String::new(),
        message: agent_status.message.clone().unwrap_or_default(),
        progress: agent_status.progress.as_ref().map(|p| ProgressInfo {
            current: p.current,
            total: p.total,
            label: if p.label.is_empty() {
                None
            } else {
                Some(p.label.clone())
            },
        }),
        last_updated_at: agent_status.last_updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::agent_status::AgentProgress;

    #[test]
    fn status_labels_cover_known_and_unknown_values() {
        assert_eq!(status_label("working"), ("RUN", "Running", "var(--accent)"));
        assert_eq!(status_label("error"), ("ERR", "Error", "var(--error)"));
        assert_eq!(status_label("other"), ("???", "Unknown", "var(--textDim)"));
    }

    #[test]
    fn status_conversion_preserves_progress_and_message() {
        let status = AgentStatus {
            pane_id: "pane-1".to_string(),
            generation: Some(1),
            status: AgentRunStatus::Working,
            message: Some("Building".to_string()),
            progress: Some(AgentProgress {
                current: 2,
                total: 5,
                label: "compile".to_string(),
            }),
            dismissed: false,
            last_updated_at: 42,
        };
        let converted = to_pane_status(&status);
        assert_eq!(converted.status, "working");
        assert_eq!(converted.message, "Building");
        assert_eq!(converted.progress.unwrap().current, 2);
    }
}
