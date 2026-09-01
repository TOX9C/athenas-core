//! Pure state transition for opening a URL in the embedded browser panel.
//!
//! The terminal link handler needs to open the right-sidebar browser and stage
//! the target URL. Keeping that mutation here — free of DOM and IPC — makes the
//! flow unit-testable without a browser or the Dioxus signal runtime.

use crate::stores::panel_manager::{PanelManagerState, RightPanel};
use crate::stores::ui::{Panel, UIState};

/// Stage the UI so the embedded browser opens in the right sidebar and lands
/// on `url`.
///
/// - `pending_browser_url` is consumed by the browser surface on its next
///   mount, so a cold-open lands directly on the link (no default-page flash).
/// - The main area is returned to `Workspace` so the sidebar actually mounts
///   the browser surface instead of showing the "Browser Relocated" hint.
/// - The right sidebar is opened with the Browser tab selected.
pub fn open_link_in_browser(ui: &mut UIState, panel: &mut PanelManagerState, url: &str) {
    ui.pending_browser_url = Some(url.to_string());
    ui.panel = Panel::Workspace;
    ui.right_sidebar_open = true;
    panel.open_right_panel(RightPanel::Browser);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_link_stages_url_and_opens_sidebar_browser() {
        let mut ui = UIState::default();
        let mut panel = PanelManagerState::new();

        open_link_in_browser(&mut ui, &mut panel, "https://example.com/docs");

        assert_eq!(
            ui.pending_browser_url.as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(ui.panel, Panel::Workspace);
        assert!(ui.right_sidebar_open);
        assert_eq!(panel.active_right_panel, RightPanel::Browser);
    }

    #[test]
    fn open_link_docks_a_main_area_browser_back_to_the_sidebar() {
        // Browser currently expanded in the main content area.
        let mut ui = UIState {
            panel: Panel::Browser,
            right_sidebar_open: false,
            ..UIState::default()
        };
        let mut panel = PanelManagerState::new();
        panel.active_right_panel = RightPanel::Assistant;

        open_link_in_browser(&mut ui, &mut panel, "http://localhost:3000");

        // Main area returns to workspace so the sidebar mounts the browser
        // surface rather than rendering the "Browser Relocated" hint.
        assert_eq!(ui.panel, Panel::Workspace);
        assert!(ui.right_sidebar_open);
        assert_eq!(panel.active_right_panel, RightPanel::Browser);
        assert_eq!(
            ui.pending_browser_url.as_deref(),
            Some("http://localhost:3000")
        );
    }

    #[test]
    fn open_link_switches_away_from_the_assistant_tab() {
        let mut ui = UIState {
            right_sidebar_open: true,
            ..UIState::default()
        };
        let mut panel = PanelManagerState::new();
        panel.active_right_panel = RightPanel::Assistant;

        open_link_in_browser(&mut ui, &mut panel, "https://github.com");

        assert_eq!(panel.active_right_panel, RightPanel::Browser);
        assert!(ui.right_sidebar_open);
    }
}
