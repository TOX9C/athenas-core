use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Whether a theme is designed for dark or light environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ThemeType {
    Dark,
    Light,
}

/// Color palette for a theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub bg: String,
    pub bg_secondary: String,
    pub bg_tertiary: String,
    pub border: String,
    pub text: String,
    pub text_muted: String,
    pub text_dim: String,
    pub accent: String,
    pub accent_hover: String,
    pub success: String,
    pub error: String,
    pub warning: String,
    pub terminal_bg: String,
    pub terminal_fg: String,
    pub terminal_cursor: String,
    pub terminal_selection: String,
}

/// Full definition of a named theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeDefinition {
    pub name: ThemeName,
    pub label: String,
    #[serde(rename = "type")]
    pub theme_type: ThemeType,
    pub colors: ThemeColors,
}

/// Available theme names. Each variant maps to the kebab-case identifier
/// used in the original TypeScript codebase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum ThemeName {
    Noir,
    Obsidian,
    Tide,
    Cedar,
    Dawn,
    Aurora,
    Erebus,
    Eclipse,
    System,
}
