use dioxus::prelude::*;

use super::browser_surface::BrowserSurface;

/// Right-sidebar browser tab.
///
/// Thin wrapper over the shared [`BrowserSurface`] in its compact (non-expanded)
/// presentation. The actual web content is a native Tauri child webview overlaid
/// on the surface's viewport; see `browser_surface.rs`.
#[component]
pub fn RightBrowserPanel() -> Element {
    rsx! {
        BrowserSurface { expanded: false }
    }
}
