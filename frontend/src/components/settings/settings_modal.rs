use super::shortcuts_ref::ShortcutsRef;
use super::theme_picker::ThemePicker;
use crate::components::shared::modal::Modal;
use crate::stores::ui::use_ui_store;
use crate::themes::AVAILABLE_FONTS;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SettingsModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn SettingsModal(props: SettingsModalProps) -> Element {
    let mut active_tab = use_signal(|| 0u8);

    let tabs = [
        "General",
        "Athena",
        "Agents",
        "Themes",
        "Shortcuts",
        "About",
    ];

    rsx! {
        Modal {
            title: "Settings",
            on_close: move |_| props.on_close.call(()),
            width: 600,

            div {
                style: "display: flex; gap: 16px; min-height: 360px;",

                div {
                    style: "width: 120px; flex-shrink: 0; display: flex; flex-direction: column; gap: 2px;",

                    for (i, tab) in tabs.iter().enumerate() {
                        {
                            let is_active = active_tab() == i as u8;
                            let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                            let color = if is_active { "var(--text)" } else { "var(--textDim)" };
                            let idx = i as u8;
                            rsx! {
                                button {
                                    key: "{tab}",
                                    style: "padding: 6px 8px; border-radius: 4px; border: none; background: {bg}; color: {color}; cursor: pointer; font-size: 11px; text-align: left;",
                                    onclick: move |_| active_tab.set(idx),
                                    "{tab}"
                                }
                            }
                        }
                    }
                }

                div {
                    style: "flex: 1; overflow-y: auto;",

                    if active_tab() == 0 {
                        GeneralSettings {}
                    }

                    if active_tab() == 1 {
                        div {
                            style: "display: flex; flex-direction: column; gap: 12px;",

                            SettingRow { label: "Default Model".to_string(), value: "claude".to_string() }
                            SettingRow { label: "Provider".to_string(), value: "anthropic".to_string() }
                            SettingRow { label: "Bypass Mode".to_string(), value: "enabled".to_string() }
                            SettingRow { label: "Auto Launch".to_string(), value: "enabled".to_string() }

                            div {
                                style: "margin-top: 8px;",
                                div {
                                    style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 6px;",
                                    "API Keys"
                                }
                                div {
                                    style: "font-size: 10px; color: var(--textDim);",
                                    "Configure API keys via environment variables"
                                }
                            }
                        }
                    }

                    if active_tab() == 2 {
                        div {
                            style: "display: flex; flex-direction: column; gap: 8px;",

                            div {
                                style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 4px;",
                                "Custom Agents"
                            }
                            div {
                                style: "font-size: 10px; color: var(--textDim);",
                                "Add custom agent shortcuts"
                            }
                            button {
                                style: "padding: 6px 12px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--text); cursor: pointer; font-size: 11px; align-self: flex-start;",
                                "+ Add Agent"
                            }
                        }
                    }

                    if active_tab() == 3 {
                        ThemePicker {}
                    }

                    if active_tab() == 4 {
                        ShortcutsRef {}
                    }

                    if active_tab() == 5 {
                        div {
                            style: "text-align: center; padding: 24px; color: var(--textDim);",
                            div { style: "font-size: 18px; font-weight: 600; color: var(--text);", "Athena" }
                            div { style: "font-size: 11px; margin-top: 4px;", "v0.1.0" }
                            div { style: "font-size: 10px; margin-top: 8px;", "AI-powered development environment" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GeneralSettings() -> Element {
    let mut ui_state = use_ui_store();
    let current_font = ui_state.read().font_family.clone();
    let current_size = ui_state.read().font_size;

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",

            div {
                div {
                    style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 8px;",
                    "Font Family"
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                    for font in AVAILABLE_FONTS {
                        {
                            let is_selected = *font == current_font;
                            let bg = if is_selected { "var(--accent)" } else { "var(--bgTertiary)" };
                            let fg = if is_selected { "var(--bg)" } else { "var(--textMuted)" };
                            let font_str = font.to_string();
                            rsx! {
                                button {
                                    key: "{font}",
                                    style: "padding: 4px 10px; border-radius: 4px; border: 1px solid var(--border); background: {bg}; color: {fg}; cursor: pointer; font-size: 11px; font-family: '{font}', monospace; transition: all 0.15s;",
                                    onclick: move |_| {
                                        ui_state.write().font_family = font_str.clone();
                                        crate::themes::apply_font_to_dom(&font_str, ui_state.read().font_size);
                                        let font_for_store = font_str.clone();
                                        spawn(async move {
                                            let _ = crate::tauri_bridge::store_set("font_family", &font_for_store).await;
                                        });
                                    },
                                    "{font}"
                                }
                            }
                        }
                    }
                }
            }

            div {
                div {
                    style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 8px;",
                    "Font Size"
                }
                div { style: "display: flex; align-items: center; gap: 12px;",
                    input {
                        r#type: "range",
                        min: "10",
                        max: "24",
                        value: "{current_size}",
                        style: "flex: 1; accent-color: var(--accent);",
                        oninput: move |e| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                ui_state.write().font_size = val;
                                crate::themes::apply_font_to_dom(&ui_state.read().font_family, val);
                                spawn(async move {
                                    let _ = crate::tauri_bridge::store_set("font_size", &val.to_string()).await;
                                });
                            }
                        },
                    }
                    span {
                        style: "font-size: 11px; color: var(--textMuted); min-width: 32px; text-align: center;",
                        "{current_size}px"
                    }
                }
            }

            div {
                style: "padding: 12px; border: 1px solid var(--border); border-radius: 6px; background: var(--bgSecondary);",
                div {
                    style: "font-size: 11px; font-weight: 600; color: var(--textMuted); margin-bottom: 8px;",
                    "Preview"
                }
                div {
                    style: "font-family: '{current_font}', monospace; font-size: {current_size}px; color: var(--text); line-height: 1.6;",
                    "fn main() {{"
                    br {}
                    "    println!(\"Hello, world!\");"
                    br {}
                    "}}"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SettingRowProps {
    label: String,
    value: String,
}

#[component]
fn SettingRow(props: SettingRowProps) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid var(--border);",
            span {
                style: "font-size: 11px; color: var(--text);",
                "{props.label}"
            }
            span {
                style: "font-size: 11px; color: var(--textDim);",
                "{props.value}"
            }
        }
    }
}
