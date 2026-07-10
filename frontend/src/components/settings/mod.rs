pub mod settings_modal;
pub mod shortcuts_ref;
pub mod theme_picker;

use dioxus::prelude::*;
use settings_modal::SettingsContent;

#[component]
pub fn SettingsPanel() -> Element {
    rsx! {
        div {
            class: "settings-panel pane-astrolabe-mark",
            style: "height: 100%; display: flex; flex-direction: column; background: var(--bg); border: 1px solid var(--border); color: var(--text);",

            // Full settings within the panel (embedded, no modal)
            div {
                style: "flex: 1; overflow: hidden;",
                SettingsContent {}
            }
        }
    }
}
