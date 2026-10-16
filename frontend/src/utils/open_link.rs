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

/// Stage a terminal link click for the open-location chooser.
///
/// Unlike `open_link_in_browser` this deliberately does NOT mutate the
/// layout: the user has not chosen where the link goes yet, and forcing the
/// sidebar open on a dismissed chooser would be a layout toggle they never
/// asked for. The chooser reads this and owns the layout change once the
/// user picks "App Browser".
pub fn request_link_open(ui: &mut UIState, url: &str) {
    ui.pending_link_request = Some(url.to_string());
}

/// Clear the staged link request after the chooser resolves (either by
/// choice or dismissal). Idempotent so a close path can call it twice.
pub fn clear_link_request(ui: &mut UIState) {
    ui.pending_link_request = None;
}

/// The host of `url` for display in the chooser, so the user can recognize
/// the target without reading a full query-string URL.
///
/// Falls back to the whole URL when it has no parsable host (a bare host, an
/// IP literal, a non-http scheme the backend will reject anyway) — the user
/// should see the real string rather than nothing.
pub fn link_display_host(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            return host.to_string();
        }
    }
    url.to_string()
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

    #[test]
    fn request_link_open_stages_without_touching_layout() {
        let mut ui = UIState::default();
        let mut panel = PanelManagerState::new();
        panel.active_right_panel = RightPanel::Assistant;

        // A dismissed chooser must not drag the sidebar open, so the request
        // staging is layout-neutral until the user picks a destination.
        request_link_open(&mut ui, "https://example.com/docs?x=1");

        assert_eq!(
            ui.pending_link_request.as_deref(),
            Some("https://example.com/docs?x=1")
        );
        assert!(!ui.right_sidebar_open);
        assert_eq!(ui.panel, Panel::Workspace);
        assert_eq!(panel.active_right_panel, RightPanel::Assistant);
        assert!(ui.pending_browser_url.is_none());
    }

    #[test]
    fn request_link_open_replaces_a_stale_request() {
        let mut ui = UIState::default();

        request_link_open(&mut ui, "https://a.example.com");
        request_link_open(&mut ui, "https://b.example.com");

        assert_eq!(
            ui.pending_link_request.as_deref(),
            Some("https://b.example.com")
        );
    }

    #[test]
    fn clear_link_request_is_idempotent() {
        let mut ui = UIState::default();
        request_link_open(&mut ui, "https://example.com");

        clear_link_request(&mut ui);
        clear_link_request(&mut ui);

        assert!(ui.pending_link_request.is_none());
    }

    #[test]
    fn link_display_host_strips_path_query_and_port() {
        assert_eq!(link_display_host("https://github.com/TOX9C"), "github.com");
        assert_eq!(
            link_display_host("https://example.com/docs?a=1&b=2"),
            "example.com"
        );
        assert_eq!(link_display_host("http://localhost:3000/x"), "localhost");
        assert_eq!(link_display_host("https://192.168.1.1:8443"), "192.168.1.1");
    }

    #[test]
    fn link_display_host_falls_back_to_the_raw_url() {
        // Unparsable or scheme-less input still shows something recognizable
        // rather than an empty host.
        assert_eq!(link_display_host("not a url"), "not a url");
        assert_eq!(link_display_host("mailto:a@b.example.com"), "mailto:a@b.example.com");
    }
}
