//! Constructors for common workspace pane configurations.

use crate::types::workspace::{AgentType, PaneConfig};
use uuid::Uuid;

/// Create a fresh shell pane with the defaults used by the workspace UI.
///
/// Use a UUID rather than wall-clock milliseconds: the top-right add button
/// can be clicked immediately after closing the last pane, and OMP/Pi can
/// create panes in the same millisecond. Reusing an id races the old PTY kill
/// and collides with the registry/session keys, leaving a header with no live
/// terminal attached.
pub fn new_shell_pane() -> PaneConfig {
    PaneConfig {
        id: format!("shell-{}", Uuid::new_v4()),
        agent_type: AgentType::Shell,
        custom_cmd: None,
        custom_agent_id: None,
        label: None,
        bypass_mode: None,
        project_name: None,
        model_name: None,
        resume_id: None,
        resume_cmd: None,
        resume_dismissed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_shell_pane_uses_shell_defaults() {
        let pane = new_shell_pane();

        assert!(pane.id.starts_with("shell-"));
        assert!(Uuid::parse_str(pane.id.strip_prefix("shell-").unwrap()).is_ok());
        assert_eq!(pane.agent_type, AgentType::Shell);
        assert_eq!(pane.custom_cmd, None);
        assert_eq!(pane.custom_agent_id, None);
        assert_eq!(pane.label, None);
        assert_eq!(pane.bypass_mode, None);
        assert_eq!(pane.project_name, None);
        assert_eq!(pane.model_name, None);
        assert_eq!(pane.resume_id, None);
        assert_eq!(pane.resume_cmd, None);
        assert_eq!(pane.resume_dismissed, None);
    }

    #[test]
    fn new_shell_panes_have_distinct_ids_when_created_back_to_back() {
        let first = new_shell_pane();
        let second = new_shell_pane();

        assert_ne!(first.id, second.id);
    }
}
