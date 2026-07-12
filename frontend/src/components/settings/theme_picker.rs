use crate::components::settings::settings_modal::GroupLabel;
use crate::stores::ui::{use_ui_store, UITheme};
use crate::themes::{get_theme, ALL_THEMES};
use dioxus::prelude::*;

#[component]
pub fn ThemePicker() -> Element {
    let dark_themes: Vec<_> = ALL_THEMES
        .iter()
        .filter(|(id, _)| !is_light_bg(&get_theme(id).bg))
        .copied()
        .collect();

    let light_themes: Vec<_> = ALL_THEMES
        .iter()
        .filter(|(id, _)| is_light_bg(&get_theme(id).bg))
        .copied()
        .collect();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 18px;",
            ThemeGroup { first: true, label: "Dark", themes: dark_themes }
            ThemeGroup { first: false, label: "Light", themes: light_themes }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ThemeGroupProps {
    first: bool,
    label: &'static str,
    themes: Vec<(&'static str, &'static str)>,
}

#[component]
fn ThemeGroup(props: ThemeGroupProps) -> Element {
    let ui_state = use_ui_store();

    rsx! {
        GroupLabel { label: props.label, first: props.first }
        div {
            style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px;",
            for (id, label) in &props.themes {
                {
                    let theme_enum = UITheme::from_name(id);
                    let is_selected = theme_enum == ui_state.read().theme;
                    let colors = get_theme(id);
                    rsx! {
                        ThemeSwatch {
                            key: "{id}",
                            id: id.to_string(),
                            label: label.to_string(),
                            bg: colors.bg.clone(),
                            accent: colors.accent.clone(),
                            bg_secondary: colors.bg_secondary.clone(),
                            is_selected,
                            theme: theme_enum,
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
    // Selection is conveyed by fill + text (footer accentSubtle tint + the
    // ACTIVE badge), not a border or halo ring. No outline, no box-shadow,
    // so the border is constant regardless of selection.
    let footer_bg = if props.is_selected { "var(--accentSubtle)" } else { "var(--bgTertiary)" };

    rsx! {
        button {
            class: "lit-sweep theme-swatch-btn",
            style: "padding: 0; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); cursor: pointer; text-align: left; overflow: hidden; transition: border-color var(--dur-fast) var(--ease);",
            onclick: move |_| {
                ui_state.write().theme = props.theme;
                apply_theme_and_persist(props.theme);
            },

            // Mini app-chrome preview
            div {
                style: "height: 48px; background: {props.bg}; position: relative; display: flex; flex-direction: column;",
                // titlebar strip
                div { style: "height: 10px; background: {props.bg_secondary}; display: flex; align-items: center; gap: 3px; padding: 0 5px;",
                    div { style: "width: 14px; height: 3px; border-radius: 2px; background: {props.accent};" }
                }
                // body: sidebar + accent bar
                div { style: "flex: 1; display: flex;",
                    div { style: "width: 16px; background: {props.bg_secondary};" }
                    div { style: "flex: 1; padding: 6px;",
                        div { style: "width: 60%; height: 4px; border-radius: 2px; background: {props.accent}; margin-bottom: 4px;" }
                        div { style: "width: 40%; height: 3px; border-radius: 2px; background: {props.bg_secondary};" }
                    }
                }
            }
            div {
                style: "padding: 8px 10px; background: {footer_bg}; display: flex; align-items: center; justify-content: space-between;",
                span {
                    style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--text); letter-spacing: 0.02em;",
                    "{props.label}"
                }
                if props.is_selected {
                    span { style: "font-size: 9px; color: var(--accent); font-weight: 700; background: var(--accentSubtle); padding: 2px 6px; border-radius: var(--radius-sm); letter-spacing: 0.04em;", "ACTIVE" }
                }
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
    wasm_bindgen_futures::spawn_local(async move {
        let _ = crate::tauri_bridge::store_set("theme", &theme_name).await;
    });
}

fn detect_system_theme() -> &'static str {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(mql)) = window.match_media("(prefers-color-scheme: light)") {
            if mql.matches() {
                return "pentelic";
            }
        }
    }
    "nyx"
}
