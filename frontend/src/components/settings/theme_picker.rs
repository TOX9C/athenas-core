use crate::stores::ui::{use_ui_store, UITheme};
use crate::themes::{get_theme, ALL_THEMES};
use dioxus::prelude::*;

#[component]
pub fn ThemePicker() -> Element {
    let ui_state = use_ui_store();

    let dark_themes: Vec<_> = ALL_THEMES
        .iter()
        .filter(|(id, _)| !is_light_bg(get_theme(id).bg))
        .copied()
        .collect();

    let light_themes: Vec<_> = ALL_THEMES
        .iter()
        .filter(|(id, _)| is_light_bg(get_theme(id).bg))
        .copied()
        .collect();

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",

            div {
                div {
                    style: "font-size: 11px; font-weight: 600; color: var(--textMuted); margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.05em;",
                    "Dark Themes"
                }
                div { style: "display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px;",
                    for (id, label) in dark_themes {
                        {
                            let theme_enum = UITheme::from_name(id);
                            let is_selected = theme_enum == ui_state.read().theme;
                            let colors = get_theme(id);
                            rsx! {
                                ThemeSwatch {
                                    key: "{id}",
                                    id: id.to_string(),
                                    label: label.to_string(),
                                    bg: colors.bg.to_string(),
                                    accent: colors.accent.to_string(),
                                    bg_secondary: colors.bg_secondary.to_string(),
                                    is_selected,
                                    theme: theme_enum,
                                }
                            }
                        }
                    }
                }
            }

            div {
                div {
                    style: "font-size: 11px; font-weight: 600; color: var(--textMuted); margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.05em;",
                    "Light Themes"
                }
                div { style: "display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px;",
                    for (id, label) in light_themes {
                        {
                            let theme_enum = UITheme::from_name(id);
                            let is_selected = theme_enum == ui_state.read().theme;
                            let colors = get_theme(id);
                            rsx! {
                                ThemeSwatch {
                                    key: "{id}",
                                    id: id.to_string(),
                                    label: label.to_string(),
                                    bg: colors.bg.to_string(),
                                    accent: colors.accent.to_string(),
                                    bg_secondary: colors.bg_secondary.to_string(),
                                    is_selected,
                                    theme: theme_enum,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn is_light_bg(bg: &str) -> bool {
    if let Some(hex) = bg.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(r) = u8::from_str_radix(&hex[0..2], 16) {
                if let Ok(g) = u8::from_str_radix(&hex[2..4], 16) {
                    if let Ok(b) = u8::from_str_radix(&hex[4..6], 16) {
                        let luminance =
                            (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0;
                        return luminance > 0.5;
                    }
                }
            }
        }
    }
    false
}

#[derive(Props, Clone, PartialEq)]
struct ThemeSwatchProps {
    id: String,
    label: String,
    bg: String,
    accent: String,
    bg_secondary: String,
    is_selected: bool,
    theme: UITheme,
}

#[component]
fn ThemeSwatch(props: ThemeSwatchProps) -> Element {
    let mut ui_state = use_ui_store();
    let border = if props.is_selected {
        "2px solid #ffffff"
    } else {
        "2px solid transparent"
    };
    let bg_box_shadow = "inset 0 0 0 1px rgba(0,0,0,0.4), 0 1px 3px rgba(0,0,0,0.2)";

    rsx! {
        button {
            style: "padding: 8px 10px; border-radius: 8px; border: {border}; background: {props.bg_secondary}; cursor: pointer; text-align: center; transition: border-color 0.15s, box-shadow 0.15s;",
            onclick: move |_| {
                ui_state.write().theme = props.theme;
                apply_theme_and_persist(props.theme);
            },

            div {
                style: "width: 36px; height: 36px; border-radius: 6px; background: {props.bg}; margin: 0 auto 6px; border: 1px solid rgba(255,255,255,0.2); display: flex; align-items: center; justify-content: center; overflow: hidden; box-shadow: {bg_box_shadow}; transition: border-color 0.15s;",
                if props.is_selected {
                    span {
                        style: "font-size: 18px; color: #ffffff; text-shadow: 0 1px 3px rgba(0,0,0,0.6); font-weight: 700; margin-top: -2px;",
                        "\u{2713}"
                    }
                } else {
                    div { style: "width: 6px; height: 6px; border-radius: 50%; background: {props.accent}; opacity: 0.8;" }
                }
            }
            span {
                style: "font-size: 10px; color: var(--text); display: block;",
                "{props.label}"
            }
        }
    }
}

pub fn apply_theme_to_dom(theme: UITheme) {
    let theme_name = if theme == UITheme::System {
        detect_system_theme()
    } else {
        theme.name()
    };

    crate::themes::apply_theme_to_dom(theme_name);
}

pub fn apply_theme_and_persist(theme: UITheme) {
    apply_theme_to_dom(theme);
    let theme_name = theme.name().to_string();
    spawn(async move {
        let _ = crate::tauri_bridge::store_set("theme", &theme_name).await;
    });
}

fn detect_system_theme() -> &'static str {
    if let Some(window) = web_sys::window() {
        let result = js_sys::Function::new_no_args(
            "window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'",
        )
        .call0(&window);
        if let Ok(val) = result {
            if let Some(s) = val.as_string() {
                if s == "light" {
                    return "dawn";
                }
            }
        }
    }
    "noir"
}
