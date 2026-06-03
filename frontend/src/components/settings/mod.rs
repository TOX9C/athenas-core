pub mod settings_modal;
pub mod shortcuts_ref;
pub mod theme_picker;

// Re-export panel
use super::settings::settings_modal::SettingsModal;
use dioxus::prelude::*;

#[component]
pub fn SettingsPanel() -> Element {
    let mut modal_open = use_signal(|| false);

    rsx! {
        div {
            class: "settings-panel",
            style: "height: 100%; display: flex; flex-direction: column; background: var(--bg); color: var(--text);",

            // Header
            div {
                style: "padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); display: flex; align-items: center; gap: 8px;",
                span {
                    style: "font-size: 13px; font-weight: 600; color: var(--text);",
                    "Settings"
                }
            }

            // Settings content (inline for panel view)
            div {
                style: "flex: 1; padding: 16px; overflow-y: auto;",

                div {
                    style: "font-size: 12px; color: var(--text); margin-bottom: 8px;",
                    "General"
                }
                div {
                    style: "font-size: 11px; color: var(--textDim);",
                    "Configure your Athena environment"
                }

                button {
                    style: "margin-top: 16px; padding: 8px 16px; border-radius: 6px; border: none; background: var(--accent); color: var(--bg); cursor: pointer; font-size: 11px;",
                    onclick: move |_| modal_open.set(true),
                    "Open Full Settings"
                }
            }

            // Full settings modal
            if modal_open() {
                SettingsModal {
                    on_close: move |_| modal_open.set(false),
                }
            }
        }
    }
}
