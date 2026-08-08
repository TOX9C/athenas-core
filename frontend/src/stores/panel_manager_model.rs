//! Pure panel activation state and transitions.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The exclusive panel that is currently frontmost in the LEFT content area.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ExclusivePanel {
    #[default]
    None,
    Athena,
    Browser,
    Editor,
}

/// The panel that is currently shown in the RIGHT sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RightPanel {
    #[default]
    None,
    Browser,
    Assistant,
    Editor,
    Skills,
}

// ---------------------------------------------------------------------------
// Panel activation logic
// ---------------------------------------------------------------------------

/// Result of applying an exclusive panel activation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelActivation {
    pub browser_open: bool,
    pub editor_open: bool,
    pub athena_open: bool,
}

/// Compute the open/closed state for each exclusive panel given an activation.
pub fn apply_activation(panel: &ExclusivePanel) -> PanelActivation {
    PanelActivation {
        browser_open: matches!(panel, ExclusivePanel::Browser),
        editor_open: matches!(panel, ExclusivePanel::Editor),
        athena_open: matches!(panel, ExclusivePanel::Athena),
    }
}

/// Determine the toggle outcome: if the requested panel is already open,
/// deactivate it; otherwise, activate it.
pub fn toggle_panel(panel: &ExclusivePanel, current: &PanelActivation) -> ExclusivePanel {
    let is_currently_open = match panel {
        ExclusivePanel::Browser => current.browser_open,
        ExclusivePanel::Editor => current.editor_open,
        ExclusivePanel::Athena => current.athena_open,
        ExclusivePanel::None => false,
    };

    if is_currently_open {
        ExclusivePanel::None
    } else {
        panel.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_activation_maps_exclusive_panels() {
        assert_eq!(
            apply_activation(&ExclusivePanel::None),
            PanelActivation::default()
        );
        assert_eq!(
            apply_activation(&ExclusivePanel::Athena),
            PanelActivation {
                browser_open: false,
                editor_open: false,
                athena_open: true
            }
        );
        assert_eq!(
            apply_activation(&ExclusivePanel::Browser),
            PanelActivation {
                browser_open: true,
                editor_open: false,
                athena_open: false
            }
        );
        assert_eq!(
            apply_activation(&ExclusivePanel::Editor),
            PanelActivation {
                browser_open: false,
                editor_open: true,
                athena_open: false
            }
        );
    }

    #[test]
    fn toggle_panel_closes_current_panel() {
        let current = apply_activation(&ExclusivePanel::Browser);
        assert_eq!(
            toggle_panel(&ExclusivePanel::Browser, &current),
            ExclusivePanel::None
        );
    }

    #[test]
    fn toggle_panel_opens_different_panel() {
        let current = apply_activation(&ExclusivePanel::Browser);
        assert_eq!(
            toggle_panel(&ExclusivePanel::Editor, &current),
            ExclusivePanel::Editor
        );
    }

    #[test]
    fn toggle_none_stays_none() {
        assert_eq!(
            toggle_panel(&ExclusivePanel::None, &PanelActivation::default()),
            ExclusivePanel::None
        );
    }
}
