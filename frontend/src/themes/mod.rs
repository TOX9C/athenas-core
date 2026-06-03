#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColors {
    pub bg: &'static str,
    pub bg_secondary: &'static str,
    pub bg_tertiary: &'static str,
    pub border: &'static str,
    pub text: &'static str,
    pub text_muted: &'static str,
    pub text_dim: &'static str,
    pub accent: &'static str,
    pub accent_hover: &'static str,
    pub success: &'static str,
    pub error: &'static str,
    pub warning: &'static str,
    pub terminal_bg: &'static str,
    pub terminal_fg: &'static str,
    pub terminal_cursor: &'static str,
    pub terminal_selection: &'static str,
}

/// ── 1. NOIR ──
/// Warm dark with amber/gold. Sophisticated, the new default.
pub fn get_theme(name: &str) -> ThemeColors {
    match name {
        "noir" => ThemeColors {
            bg: "#0f0d0b",
            bg_secondary: "#1a1815",
            bg_tertiary: "#242220",
            border: "#33302c",
            text: "#e8e4df",
            text_muted: "#a09890",
            text_dim: "#685e56",
            accent: "#e5a443",
            accent_hover: "#f0b860",
            success: "#4ade80",
            error: "#ef4444",
            warning: "#f59e0b",
            terminal_bg: "#0f0d0b",
            terminal_fg: "#e8e4df",
            terminal_cursor: "#e5a443",
            terminal_selection: "rgba(229, 164, 67, 0.3)",
        },
        "obsidian" => ThemeColors {
            bg: "#080a0f",
            bg_secondary: "#0f1218",
            bg_tertiary: "#181c24",
            border: "#252b35",
            text: "#d8dce6",
            text_muted: "#828c9c",
            text_dim: "#4e5666",
            accent: "#8b5cf6",
            accent_hover: "#a78bfa",
            success: "#4ade80",
            error: "#f87171",
            warning: "#fbbf24",
            terminal_bg: "#080a0f",
            terminal_fg: "#d8dce6",
            terminal_cursor: "#8b5cf6",
            terminal_selection: "rgba(139, 92, 246, 0.3)",
        },
        "tide" => ThemeColors {
            bg: "#071218",
            bg_secondary: "#0e1c24",
            bg_tertiary: "#192832",
            border: "#253845",
            text: "#cfe0e8",
            text_muted: "#8ea8b5",
            text_dim: "#4f6a76",
            accent: "#ff7b6b",
            accent_hover: "#ff9b8f",
            success: "#4ade80",
            error: "#f87171",
            warning: "#fbbf24",
            terminal_bg: "#071218",
            terminal_fg: "#cfe0e8",
            terminal_cursor: "#ff7b6b",
            terminal_selection: "rgba(255, 123, 107, 0.3)",
        },
        "cedar" => ThemeColors {
            bg: "#0a1610",
            bg_secondary: "#122218",
            bg_tertiary: "#1e3024",
            border: "#2a4030",
            text: "#d4e0c8",
            text_muted: "#88a078",
            text_dim: "#4a6045",
            accent: "#48d9a6",
            accent_hover: "#71f7c2",
            success: "#22c55e",
            error: "#ef4444",
            warning: "#f59e0b",
            terminal_bg: "#0a1610",
            terminal_fg: "#d4e0c8",
            terminal_cursor: "#48d9a6",
            terminal_selection: "rgba(72, 217, 166, 0.3)",
        },
        "dawn" => ThemeColors {
            bg: "#f5f0eb",
            bg_secondary: "#e8e2da",
            bg_tertiary: "#dbd3ca",
            border: "#c5bdb3",
            text: "#2a2520",
            text_muted: "#756e66",
            text_dim: "#a09890",
            accent: "#c45c26",
            accent_hover: "#e06d30",
            success: "#2f8a4c",
            error: "#c0392b",
            warning: "#c8892e",
            terminal_bg: "#f5f0eb",
            terminal_fg: "#2a2520",
            terminal_cursor: "#c45c26",
            terminal_selection:"rgba(196, 92, 38, 0.2)",
        },
        _ => get_theme("noir"),
    }
}

pub const ALL_THEMES: &[(&str, &str)] = &[
    ("noir", "Noir"),
    ("obsidian", "Obsidian"),
    ("tide", "Tide"),
    ("cedar", "Cedar"),
    ("dawn", "Dawn"),
];

pub const AVAILABLE_FONTS: &[&str] = &[
    "JetBrains Mono",
    "Fira Code",
    "Cascadia Code",
    "Source Code Pro",
    "IBM Plex Mono",
    "Hack",
    "SF Mono",
    "Menlo",
    "Consolas",
    "Monaco",
];

fn set_css_property(property: &str, value: &str) {
    let safe_value = value.replace('\'', "\\'").replace('"', "\\\"");
    let script = format!(
        "document.documentElement.style.setProperty('{}', '{}');",
        property, safe_value
    );
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Function::new_no_args(&script).call0(&window);
    }
}

fn set_data_theme(value: &str) {
    let safe_value = value.replace('\'', "\\'").replace('"', "\\\"");
    let script = format!(
        "document.documentElement.setAttribute('data-theme', '{}');",
        safe_value
    );
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Function::new_no_args(&script).call0(&window);
    }
}

pub fn apply_theme_to_dom(theme_name: &str) {
    let colors = get_theme(theme_name);
    set_data_theme(theme_name);
    set_css_property("--bg", colors.bg);
    set_css_property("--bgSecondary", colors.bg_secondary);
    set_css_property("--bgTertiary", colors.bg_tertiary);
    set_css_property("--border", colors.border);
    set_css_property("--text", colors.text);
    set_css_property("--textMuted", colors.text_muted);
    set_css_property("--textDim", colors.text_dim);
    set_css_property("--accent", colors.accent);
    set_css_property("--accentHover", colors.accent_hover);
    set_css_property("--success", colors.success);
    set_css_property("--error", colors.error);
    set_css_property("--warning", colors.warning);
    set_css_property("--terminalBg", colors.terminal_bg);
    set_css_property("--terminalFg", colors.terminal_fg);
    set_css_property("--terminalCursor", colors.terminal_cursor);
    set_css_property("--terminalSelection", colors.terminal_selection);
}

pub fn apply_font_to_dom(font_family: &str, font_size: u8) {
    set_css_property("--fontFamily", &format!("'{}', monospace", font_family));
    set_css_property("--fontSize", &format!("{}px", font_size));
}
