use dioxus::prelude::*;

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

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global panel manager state.
#[derive(Clone, PartialEq, Default)]
pub struct PanelManagerState {
    pub active_panel: ExclusivePanel,
    pub active_right_panel: RightPanel,
    pub right_panel_width_percent: f32,
}

impl PanelManagerState {
    pub fn new() -> Self {
        Self {
            active_panel: ExclusivePanel::None,
            active_right_panel: RightPanel::None,
            right_panel_width_percent: 35.0,
        }
    }

    /// Activate a left-panel exclusively.
    pub fn activate(&mut self, panel: ExclusivePanel) {
        self.active_panel = panel;
    }

    /// Toggle a left-panel: activate if not current, deactivate if current.
    pub fn toggle(&mut self, panel: &ExclusivePanel) {
        let current = apply_activation(&self.active_panel);
        self.active_panel = toggle_panel(panel, &current);
    }

    /// Get the derived open/closed state for each exclusive panel.
    pub fn activation(&self) -> PanelActivation {
        apply_activation(&self.active_panel)
    }

    /// Toggle a right sidebar panel. If the requested panel is already active
    /// and the sidebar is open, close it. Otherwise switch to the panel and
    /// ensure the sidebar is open. Returns the desired sidebar open state.
    pub fn toggle_right_panel(&mut self, panel: RightPanel, currently_open: bool) -> bool {
        if self.active_right_panel == panel && currently_open {
            false
        } else {
            self.active_right_panel = panel;
            true
        }
    }

    /// Open a specific right sidebar panel. Unlike `toggle_right_panel`,
    /// this unconditionally sets the active panel without ever closing it.
    pub fn open_right_panel(&mut self, panel: RightPanel) {
        self.active_right_panel = panel;
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the panel manager signal from the Dioxus context.
pub fn use_panel_manager_store() -> Signal<PanelManagerState> {
    use_context::<Signal<PanelManagerState>>()
}

/// Initialize the panel manager store as a context provider.
pub fn provide_panel_manager_store() {
    use_context_provider(|| Signal::new(PanelManagerState::new()));
}
