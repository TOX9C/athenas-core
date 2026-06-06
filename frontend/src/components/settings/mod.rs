pub mod settings_modal;
pub mod shortcuts_ref;
pub mod theme_picker;

use dioxus::prelude::*;
use settings_modal::SettingsContent;

#[component]
pub fn SettingsPanel() -> Element {
    rsx! {
        div {
            class: "settings-panel",
            style: "height: 100%; display: flex; flex-direction: column; background: var(--bg); color: var(--text);",

            // Full settings within the panel (embedded, no modal)
            div {
                style: "flex: 1; overflow: hidden;",
                SettingsContent {}
            }
        }
    }
}
