//! Pure theme names, contracts, and palette definitions.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

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
#[derive(Default)]
pub enum ThemeName {
    #[default]
    Nyx,
    Aegis,
    Erebus,
    Pentelic,
    Olive,
    Sky,
    System,
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
        // ── Nyx — neutral near-black + gold (default dark).
        "nyx" => ThemeColors {
            bg: "#0B0B0C".into(),
            bg_secondary: "#111114".into(),
            bg_tertiary: "#16171A".into(),
            bg_hover: "#1C1D21".into(),
            border: "#1F2024".into(),
            border_active: "#C9A24B".into(),
            text: "#E6E6E6".into(),
            text_muted: "#8A8A90".into(),
            text_dim: "#5A5A60".into(),
            accent: "#C9A24B".into(),
            accent_hover: "#E2C26C".into(),
            accent_subtle: "rgba(201, 162, 75, 0.10)".into(),
            accent_teal: "#4FA39E".into(),
            success: "#7BAE5A".into(),
            error: "#C5654D".into(),
            warning: "#D2973C".into(),
            terminal_bg: "#0B0B0C".into(),
            terminal_fg: "#E6E6E6".into(),
            terminal_cursor: "#C9A24B".into(),
            terminal_selection: "rgba(201, 162, 75, 0.18)".into(),
            glow_color: "transparent".into(),
            noise_opacity: 0.0,
        },
        // ── Aegis — second dark (slightly cooler-neutral, gold).
        "aegis" => ThemeColors {
            bg: "#0C0E12".into(),
            bg_secondary: "#121420".into(),
            bg_tertiary: "#181B26".into(),
            bg_hover: "#1E222E".into(),
            border: "#232733".into(),
            border_active: "#C9A24B".into(),
            text: "#E8EAEE".into(),
            text_muted: "#8C909A".into(),
            text_dim: "#5C606C".into(),
            accent: "#CBA84E".into(),
            accent_hover: "#E2C26C".into(),
            accent_subtle: "rgba(203, 168, 78, 0.10)".into(),
            accent_teal: "#4FA39E".into(),
            success: "#7BAE5A".into(),
            error: "#C5654D".into(),
            warning: "#D2973C".into(),
            terminal_bg: "#0C0E12".into(),
            terminal_fg: "#E8EAEE".into(),
            terminal_cursor: "#CBA84E".into(),
            terminal_selection: "rgba(203, 168, 78, 0.18)".into(),
            glow_color: "transparent".into(),
            noise_opacity: 0.0,
        },
        // ── Erebus — deepest black (electrum-gold).
        "erebus" => ThemeColors {
            bg: "#070708".into(),
            bg_secondary: "#0E0E10".into(),
            bg_tertiary: "#141417".into(),
            bg_hover: "#1A1A1E".into(),
            border: "#1E1E22".into(),
            border_active: "#E2C26C".into(),
            text: "#ECECEE".into(),
            text_muted: "#8E8E94".into(),
            text_dim: "#5C5C64".into(),
            accent: "#E2C26C".into(),
            accent_hover: "#F0D890".into(),
            accent_subtle: "rgba(226, 194, 108, 0.10)".into(),
            accent_teal: "#4FA39E".into(),
            success: "#7BAE5A".into(),
            error: "#C5654D".into(),
            warning: "#D2973C".into(),
            terminal_bg: "#070708".into(),
            terminal_fg: "#ECECEE".into(),
            terminal_cursor: "#E2C26C".into(),
            terminal_selection: "rgba(226, 194, 108, 0.18)".into(),
            glow_color: "transparent".into(),
            noise_opacity: 0.0,
        },
        // ── Pentelic — solstice marble (light, inked-bronze accent).
        "pentelic" => ThemeColors {
            bg: "#F5F2EA".into(),
            bg_secondary: "#EFEBE0".into(),
            bg_tertiary: "#E8E3D4".into(),
            bg_hover: "#DFD9C8".into(),
            border: "#DDD5C2".into(),
            border_active: "#A8742F".into(),
            text: "#241F16".into(),
            text_muted: "#6A6453".into(),
            text_dim: "#9A9484".into(),
            accent: "#A8742F".into(),
            accent_hover: "#C08A40".into(),
            accent_subtle: "rgba(168, 116, 47, 0.10)".into(),
            accent_teal: "#2F7E79".into(),
            success: "#3E7A33".into(),
            error: "#B14530".into(),
            warning: "#B0791E".into(),
            terminal_bg: "#F5F2EA".into(),
            terminal_fg: "#241F16".into(),
            terminal_cursor: "#A8742F".into(),
            terminal_selection: "rgba(168, 116, 47, 0.16)".into(),
            glow_color: "transparent".into(),
            noise_opacity: 0.0,
        },
        // ── Olive — solstice parchment (warm light).
        "olive" => ThemeColors {
            bg: "#F2EFDF".into(),
            bg_secondary: "#EBE8D6".into(),
            bg_tertiary: "#E3DFC9".into(),
            bg_hover: "#D9D4BD".into(),
            border: "#D6D1B9".into(),
            border_active: "#8A7320".into(),
            text: "#252119".into(),
            text_muted: "#6B6550".into(),
            text_dim: "#9C9578".into(),
            accent: "#8A7320".into(),
            accent_hover: "#A2882C".into(),
            accent_subtle: "rgba(138, 115, 32, 0.10)".into(),
            accent_teal: "#2F7E79".into(),
            success: "#4A7A2C".into(),
            error: "#B14530".into(),
            warning: "#A8791E".into(),
            terminal_bg: "#F2EFDF".into(),
            terminal_fg: "#252119".into(),
            terminal_cursor: "#8A7320".into(),
            terminal_selection: "rgba(138, 115, 32, 0.16)".into(),
            glow_color: "transparent".into(),
            noise_opacity: 0.0,
        },
        // ── Sky — solstice mist (cool light).
        "sky" => ThemeColors {
            bg: "#EFF2F6".into(),
            bg_secondary: "#E7EBF1".into(),
            bg_tertiary: "#DDE4ED".into(),
            bg_hover: "#D2DBE5".into(),
            border: "#D6DCE6".into(),
            border_active: "#1F6F8B".into(),
            text: "#16202E".into(),
            text_muted: "#566070".into(),
            text_dim: "#8A93A2".into(),
            accent: "#1F6F8B".into(),
            accent_hover: "#2A86A6".into(),
            accent_subtle: "rgba(31, 111, 139, 0.10)".into(),
            accent_teal: "#1F6F8B".into(),
            success: "#2F7D45".into(),
            error: "#B14536".into(),
            warning: "#B07A1E".into(),
            terminal_bg: "#EFF2F6".into(),
            terminal_fg: "#16202E".into(),
            terminal_cursor: "#1F6F8B".into(),
            terminal_selection: "rgba(31, 111, 139, 0.16)".into(),
            glow_color: "transparent".into(),
            noise_opacity: 0.0,
        },
        _ => get_theme("nyx"),
    }
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
    "JetBrainsMono Nerd Font",
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
