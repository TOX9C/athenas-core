use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use wasm_bindgen::JsCast;

/// Whether a theme is designed for dark or light environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ThemeType {
    Dark,
    Light,
}

/// Color palette for a theme. Drives every CSS custom property in the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub bg: String,
    pub bg_secondary: String,
    pub bg_tertiary: String,
    pub bg_hover: String,
    pub border: String,
    pub border_active: String,
    pub text: String,
    pub text_muted: String,
    pub text_dim: String,
    pub accent: String,
    pub accent_hover: String,
    pub accent_subtle: String,
    pub accent_teal: String,
    pub success: String,
    pub error: String,
    pub warning: String,
    pub terminal_bg: String,
    pub terminal_fg: String,
    pub terminal_cursor: String,
    pub terminal_selection: String,
    /// Atmosphere — lamp glow color (rgba) + grain opacity.
    pub glow_color: String,
    pub noise_opacity: f32,
}

/// Full definition of a named theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeDefinition {
    pub name: ThemeName,
    pub label: String,
    #[serde(rename = "type")]
    pub theme_type: ThemeType,
    pub colors: ThemeColors,
}

/// Available theme names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum ThemeName {
    Nyx,
    Aegis,
    Erebus,
    Pentelic,
    Olive,
    Sky,
    System,
}

impl Default for ThemeName {
    fn default() -> Self {
        ThemeName::Nyx
    }
}

impl ThemeName {
    pub fn all() -> &'static [(Self, &'static str)] {
        &[
            (ThemeName::Nyx, "Nyx"),
            (ThemeName::Aegis, "Aegis"),
            (ThemeName::Erebus, "Erebus"),
            (ThemeName::Pentelic, "Pentelic"),
            (ThemeName::Olive, "Olive"),
            (ThemeName::Sky, "Sky"),
            (ThemeName::System, "System"),
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ThemeName::Nyx => "Nyx",
            ThemeName::Aegis => "Aegis",
            ThemeName::Erebus => "Erebus",
            ThemeName::Pentelic => "Pentelic",
            ThemeName::Olive => "Olive",
            ThemeName::Sky => "Sky",
            ThemeName::System => "System",
        }
    }

    pub fn is_dark(&self) -> bool {
        matches!(self, ThemeName::Nyx | ThemeName::Aegis | ThemeName::Erebus)
    }
}

/// Resolve a palette by its lowercase id. Falls back to Nyx.
pub fn get_theme(name: &str) -> ThemeColors {
    match name {
        // ── Nyx — obsidian + bronze-gold (default). The lamp in the dark temple.
        "nyx" => ThemeColors {
            bg: "#0E0E11".into(),
            bg_secondary: "#16161A".into(),
            bg_tertiary: "#1E1E23".into(),
            bg_hover: "#26262C".into(),
            border: "#2A2A31".into(),
            border_active: "#3A3A43".into(),
            text: "#ECEAE3".into(),
            text_muted: "#9A968C".into(),
            text_dim: "#6A675E".into(),
            accent: "#C9A24B".into(),
            accent_hover: "#E0BC6A".into(),
            accent_subtle: "rgba(201, 162, 75, 0.12)".into(),
            accent_teal: "#4FA39E".into(),
            success: "#7BAE5A".into(),
            error: "#C5654D".into(),
            warning: "#D2973C".into(),
            terminal_bg: "#0E0E11".into(),
            terminal_fg: "#ECEAE3".into(),
            terminal_cursor: "#C9A24B".into(),
            terminal_selection: "rgba(201, 162, 75, 0.22)".into(),
            glow_color: "rgba(201, 162, 75, 0.10)".into(),
            noise_opacity: 0.022,
        },
        // ── Aegis — deep Aegean blue-black + bronze, faint teal glow.
        "aegis" => ThemeColors {
            bg: "#0A0F18".into(),
            bg_secondary: "#111826".into(),
            bg_tertiary: "#18202F".into(),
            bg_hover: "#1F2940".into(),
            border: "#243044".into(),
            border_active: "#33425C".into(),
            text: "#E6ECF2".into(),
            text_muted: "#8696AC".into(),
            text_dim: "#5A6678".into(),
            accent: "#CBA257".into(),
            accent_hover: "#E2BD78".into(),
            accent_subtle: "rgba(203, 162, 87, 0.13)".into(),
            accent_teal: "#56B0AA".into(),
            success: "#6FAE7A".into(),
            error: "#C5654D".into(),
            warning: "#D2973C".into(),
            terminal_bg: "#0A0F18".into(),
            terminal_fg: "#E6ECF2".into(),
            terminal_cursor: "#CBA257".into(),
            terminal_selection: "rgba(86, 176, 170, 0.22)".into(),
            glow_color: "rgba(86, 176, 170, 0.10)".into(),
            noise_opacity: 0.02,
        },
        // ── Erebus — true black + gold leaf. Maximum contrast, minimal light.
        "erebus" => ThemeColors {
            bg: "#060607".into(),
            bg_secondary: "#0E0E10".into(),
            bg_tertiary: "#161618".into(),
            bg_hover: "#1E1E20".into(),
            border: "#232325".into(),
            border_active: "#343437".into(),
            text: "#EDEAE0".into(),
            text_muted: "#8E8A80".into(),
            text_dim: "#5E5A52".into(),
            accent: "#D8B765".into(),
            accent_hover: "#ECD089".into(),
            accent_subtle: "rgba(216, 183, 101, 0.12)".into(),
            accent_teal: "#4FA39E".into(),
            success: "#7BAE5A".into(),
            error: "#C5654D".into(),
            warning: "#D2973C".into(),
            terminal_bg: "#060607".into(),
            terminal_fg: "#EDEAE0".into(),
            terminal_cursor: "#D8B765".into(),
            terminal_selection: "rgba(216, 183, 101, 0.22)".into(),
            glow_color: "rgba(216, 183, 101, 0.08)".into(),
            noise_opacity: 0.026,
        },
        // ── Pentelic — Pentelic marble + ink + terracotta-bronze.
        "pentelic" => ThemeColors {
            bg: "#F6F4EE".into(),
            bg_secondary: "#EFECE3".into(),
            bg_tertiary: "#E6E2D6".into(),
            bg_hover: "#DCD7C8".into(),
            border: "#DAD5C7".into(),
            border_active: "#C3BCA8".into(),
            text: "#211E18".into(),
            text_muted: "#6A6456".into(),
            text_dim: "#9A9484".into(),
            accent: "#A8742F".into(),
            accent_hover: "#C08A40".into(),
            accent_subtle: "rgba(168, 116, 47, 0.14)".into(),
            accent_teal: "#2F7E79".into(),
            success: "#3E7A33".into(),
            error: "#B14530".into(),
            warning: "#B0791E".into(),
            terminal_bg: "#F6F4EE".into(),
            terminal_fg: "#211E18".into(),
            terminal_cursor: "#A8742F".into(),
            terminal_selection: "rgba(168, 116, 47, 0.16)".into(),
            glow_color: "rgba(168, 116, 47, 0.06)".into(),
            noise_opacity: 0.016,
        },
        // ── Olive — warm parchment + olive-gold + bronze.
        "olive" => ThemeColors {
            bg: "#F3F1E7".into(),
            bg_secondary: "#EBE8DB".into(),
            bg_tertiary: "#E1DDCC".into(),
            bg_hover: "#D6D1BC".into(),
            border: "#D3CDB8".into(),
            border_active: "#BDB69C".into(),
            text: "#232117".into(),
            text_muted: "#6B6550".into(),
            text_dim: "#9C9578".into(),
            accent: "#8A7320".into(),
            accent_hover: "#A2882C".into(),
            accent_subtle: "rgba(138, 115, 32, 0.14)".into(),
            accent_teal: "#2F7E79".into(),
            success: "#4A7A2C".into(),
            error: "#B14530".into(),
            warning: "#A8791E".into(),
            terminal_bg: "#F3F1E7".into(),
            terminal_fg: "#232117".into(),
            terminal_cursor: "#8A7320".into(),
            terminal_selection: "rgba(138, 115, 32, 0.16)".into(),
            glow_color: "rgba(138, 115, 32, 0.06)".into(),
            noise_opacity: 0.016,
        },
        // ── Sky — cool marble + Aegean teal (the one cool light theme).
        "sky" => ThemeColors {
            bg: "#F7F9FB".into(),
            bg_secondary: "#EEF1F5".into(),
            bg_tertiary: "#E3E8EF".into(),
            bg_hover: "#D7DEE8".into(),
            border: "#D6DCE4".into(),
            border_active: "#BCC6D2".into(),
            text: "#16202E".into(),
            text_muted: "#566070".into(),
            text_dim: "#8A93A2".into(),
            accent: "#1F6F8B".into(),
            accent_hover: "#2A86A6".into(),
            accent_subtle: "rgba(31, 111, 139, 0.12)".into(),
            accent_teal: "#1F6F8B".into(),
            success: "#2F7D45".into(),
            error: "#B14536".into(),
            warning: "#B07A1E".into(),
            terminal_bg: "#F7F9FB".into(),
            terminal_fg: "#16202E".into(),
            terminal_cursor: "#1F6F8B".into(),
            terminal_selection: "rgba(31, 111, 139, 0.16)".into(),
            glow_color: "rgba(31, 111, 139, 0.07)".into(),
            noise_opacity: 0.014,
        },
        _ => get_theme("nyx"),
    }
}

/// Apply a palette to the document root as CSS custom properties, including the
/// atmosphere tokens (lamp glow + grain). Unlike the previous engine, this drives
/// the full token set so light/dark themes carry correct hover/border/atmosphere.
pub fn apply_theme_to_dom(theme_name: &str) {
    let c = get_theme(theme_name);
    set_data_theme(theme_name);

    let derived_ring = c
        .accent_subtle
        .replace("0.12", "0.55")
        .replace("0.13", "0.55")
        .replace("0.14", "0.55");
    let props: [(&str, &str); 23] = [
        ("--bg", &c.bg),
        ("--bgSecondary", &c.bg_secondary),
        ("--bgTertiary", &c.bg_tertiary),
        ("--bgHover", &c.bg_hover),
        ("--border", &c.border),
        ("--borderActive", &c.border_active),
        ("--text", &c.text),
        ("--textMuted", &c.text_muted),
        ("--textDim", &c.text_dim),
        ("--accent", &c.accent),
        ("--accentHover", &c.accent_hover),
        ("--accentSubtle", &c.accent_subtle),
        ("--accentTeal", &c.accent_teal),
        ("--ring", &derived_ring),
        ("--success", &c.success),
        ("--error", &c.error),
        ("--warning", &c.warning),
        ("--terminalBg", &c.terminal_bg),
        ("--terminalFg", &c.terminal_fg),
        ("--terminalCursor", &c.terminal_cursor),
        ("--terminalSelection", &c.terminal_selection),
        ("--themeGlowColor", &c.glow_color),
        ("--themeGlowOpacity", "1"),
    ];
    for (k, v) in props {
        set_css_property(k, v);
    }
    set_css_property("--themeNoiseOpacity", &format!("{}", c.noise_opacity));
}

/// Set a CSS custom property on the document root via typed DOM bindings.
///
/// This used to build a JS string and run it through `js_sys::Function::new_no_args`
/// (i.e. `new Function(code)`), which the JS engine treats as `eval`. The app's
/// Content-Security-Policy only allows `'self' 'wasm-unsafe-eval'`, so that threw
/// `EvalError: Refused to evaluate a string as JavaScript` on every theme apply —
/// including the mount-time apply, which aborted the Dioxus runtime with a
/// `RefCell already borrowed` panic and left the UI unable to re-render.
/// `CssStyleDeclaration::set_property` performs the identical DOM mutation but
/// never invokes the JS parser, so it needs no CSP relaxation.
fn set_css_property(property: &str, value: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(html_el) = document.document_element() else {
        return;
    };
    // `style` is defined on `HtmlElement`, not `Element`; `document_element`
    // returns the latter, so downcast before accessing the inline style.
    let Some(html) = html_el.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let style = html.style();
    let _ = style.set_property(property, value);
}

/// Set `data-theme` on the document root. Same reasoning as `set_css_property`:
/// use `Element::set_attribute` instead of `eval`-ing a JS string.
fn set_data_theme(value: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(html) = document.document_element() else {
        return;
    };
    let _ = html.set_attribute("data-theme", value);
}

/// Apply the user-overridable mono font + base size. Display/UI families are fixed
/// in CSS; only the terminal/code mono face is user-configurable.
pub fn apply_font_to_dom(font_family: &str, font_size: u8) {
    set_css_property(
        "--fontFamily",
        &format!(
            "'{}', 'Monaspace Neon', ui-monospace, monospace",
            font_family
        ),
    );
    set_css_property("--fontSize", &format!("{}px", font_size));
}

pub const ALL_THEMES: &[(&str, &str)] = &[
    ("nyx", "Nyx"),
    ("aegis", "Aegis"),
    ("erebus", "Erebus"),
    ("pentelic", "Pentelic"),
    ("olive", "Olive"),
    ("sky", "Sky"),
];

pub const AVAILABLE_FONTS: &[&str] = &[
    "Monaspace Neon",
    "JetBrains Mono",
    "Fira Code",
    "Cascadia Code",
    "Source Code Pro",
    "IBM Plex Mono",
    "Hack",
    "SF Mono",
    "Menlo",
    "Consolas",
];
