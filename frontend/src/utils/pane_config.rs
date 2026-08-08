//! Constructors for common workspace pane configurations.

use crate::types::workspace::{AgentType, PaneConfig};
use crate::utils::time::now_ms;

/// Create a fresh shell pane with the defaults used by the workspace UI.
///
/// The timestamp-based identifier preserves the existing frontend behavior
/// while keeping both shell-pane entry points in sync.
pub fn new_shell_pane() -> PaneConfig {
    PaneConfig {
        id: format!("{:x}-sh", now_ms()),
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

        assert!(pane.id.ends_with("-sh"));
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
}
