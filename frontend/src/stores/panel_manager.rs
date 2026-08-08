use dioxus::prelude::*;

#[path = "panel_manager_model.rs"]
mod panel_manager_model;

pub use panel_manager_model::{
    apply_activation, toggle_panel, ExclusivePanel, PanelActivation, RightPanel,
};

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
