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

#[derive(Debug, Clone, Copy)]
pub struct ThemeExtras {
    bg_atmosphere: &'static str,
    glow_opacity: &'static str,
    noise_opacity: &'static str,
    glow_color: &'static str,
}

/// ── 1. ONYX ──
/// Deep charcoal with soft blue. Clean, refined, the new default.
pub fn get_theme(name: &str) -> ThemeColors {
    match name {
        "onyx" => ThemeColors {
            bg: "#0D0D0E",
            bg_secondary: "#161718",
            bg_tertiary: "#202123",
            border: "#2A2C2F",
            text: "#F0F0F2",
            text_muted: "#9CA3AF",
            text_dim: "#5E646E",
            accent: "#60A5FA",
            accent_hover: "#93C5FD",
            success: "#34D399",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#0D0D0E",
            terminal_fg: "#F0F0F2",
            terminal_cursor: "#60A5FA",
            terminal_selection: "rgba(96, 165, 250, 0.25)",
        },
        "sage" => ThemeColors {
            bg: "#12140F",
            bg_secondary: "#1B1E16",
            bg_tertiary: "#252820",
            border: "#33362C",
            text: "#EDF0E6",
            text_muted: "#A3B18A",
            text_dim: "#5E6860",
            accent: "#8FA870",
            accent_hover: "#B9D39A",
            success: "#4ADE80",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#12140F",
            terminal_fg: "#EDF0E6",
            terminal_cursor: "#8FA870",
            terminal_selection: "rgba(143, 168, 112, 0.25)",
        },
        "volcanic" => ThemeColors {
            bg: "#14100F",
            bg_secondary: "#1E1916",
            bg_tertiary: "#28221E",
            border: "#38322D",
            text: "#F5EDE8",
            text_muted: "#9C887A",
            text_dim: "#5C544E",
            accent: "#E66A3C",
            accent_hover: "#F49B6E",
            success: "#4ADE80",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#14100F",
            terminal_fg: "#F5EDE8",
            terminal_cursor: "#E66A3C",
            terminal_selection: "rgba(230, 106, 60, 0.25)",
        },
        "tidal" => ThemeColors {
            bg: "#0C1218",
            bg_secondary: "#161E26",
            bg_tertiary: "#1E2832",
            border: "#2D3845",
            text: "#E8F0F5",
            text_muted: "#8EA4B8",
            text_dim: "#546878",
            accent: "#2DD4BF",
            accent_hover: "#5EEDD8",
            success: "#4ADE80",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#0C1218",
            terminal_fg: "#E8F0F5",
            terminal_cursor: "#2DD4BF",
            terminal_selection: "rgba(45, 212, 191, 0.25)",
        },
        "obsidian" => ThemeColors {
            bg: "#111215",
            bg_secondary: "#1A1B20",
            bg_tertiary: "#24252D",
            border: "#313238",
            text: "#E8E8ED",
            text_muted: "#9CA3AF",
            text_dim: "#5E646E",
            accent: "#A78BFA",
            accent_hover: "#C4B5FD",
            success: "#34D399",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#111215",
            terminal_fg: "#E8E8ED",
            terminal_cursor: "#A78BFA",
            terminal_selection: "rgba(167, 139, 250, 0.25)",
        },
        "grove" => ThemeColors {
            bg: "#0E130F",
            bg_secondary: "#171B16",
            bg_tertiary: "#212820",
            border: "#2D3328",
            text: "#EDF2ED",
            text_muted: "#9AAF9C",
            text_dim: "#5E7870",
            accent: "#6EE7B7",
            accent_hover: "#A7F3D0",
            success: "#34D399",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#0E130F",
            terminal_fg: "#EDF2ED",
            terminal_cursor: "#6EE7B7",
            terminal_selection: "rgba(110, 231, 183, 0.25)",
        },
        "midnight" => ThemeColors {
            bg: "#0B0F1E",
            bg_secondary: "#12182C",
            bg_tertiary: "#1B2238",
            border: "#2A3352",
            text: "#E8EEF4",
            text_muted: "#8998B8",
            text_dim: "#546072",
            accent: "#818CF8",
            accent_hover: "#A5B4FC",
            success: "#34D399",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#0B0F1E",
            terminal_fg: "#E8EEF4",
            terminal_cursor: "#818CF8",
            terminal_selection: "rgba(129, 140, 248, 0.25)",
        },
        "ember" => ThemeColors {
            bg: "#12100E",
            bg_secondary: "#1E1B16",
            bg_tertiary: "#2A261E",
            border: "#38332A",
            text: "#F0ECDC",
            text_muted: "#BFAE8A",
            text_dim: "#6E5E4E",
            accent: "#F59E69",
            accent_hover: "#FDBA74",
            success: "#22C55E",
            error: "#F87171",
            warning: "#F59E0B",
            terminal_bg: "#12100E",
            terminal_fg: "#F0ECDC",
            terminal_cursor: "#F59E69",
            terminal_selection: "rgba(245, 158, 105, 0.25)",
        },
        "ivory" => ThemeColors {
            bg: "#FDFBF7",
            bg_secondary: "#F5F2EE",
            bg_tertiary: "#EDEAE4",
            border: "#D5D0C8",
            text: "#1C1914",
            text_muted: "#6B6258",
            text_dim: "#9E968C",
            accent: "#0D9488",
            accent_hover: "#14B8A6",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#FDFBF7",
            terminal_fg: "#1C1914",
            terminal_cursor: "#0D9488",
            terminal_selection: "rgba(13, 148, 136, 0.18)",
        },
        "sand" => ThemeColors {
            bg: "#F8F6F1",
            bg_secondary: "#F0EEE8",
            bg_tertiary: "#E6E4DD",
            border: "#D5D2CC",
            text: "#1F1C18",
            text_muted: "#756D63",
            text_dim: "#A0988E",
            accent: "#D97706",
            accent_hover: "#B45309",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#F8F6F1",
            terminal_fg: "#1F1C18",
            terminal_cursor: "#D97706",
            terminal_selection: "rgba(217, 119, 6, 0.18)",
        },
        "ash" => ThemeColors {
            bg: "#EEEDEB",
            bg_secondary: "#E6E5E3",
            bg_tertiary: "#DDDCDA",
            border: "#C8C7C4",
            text: "#1A1A1A",
            text_muted: "#605E5C",
            text_dim: "#908E8C",
            accent: "#374151",
            accent_hover: "#1F2937",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#EEEDEB",
            terminal_fg: "#1A1A1A",
            terminal_cursor: "#374151",
            terminal_selection: "rgba(55, 65, 81, 0.18)",
        },
        "rose" => ThemeColors {
            bg: "#FDF2F0",
            bg_secondary: "#F5E8E6",
            bg_tertiary: "#EDDEDC",
            border: "#D5B8B5",
            text: "#1A1210",
            text_muted: "#785A5A",
            text_dim: "#A08282",
            accent: "#E76F8B",
            accent_hover: "#C43E5C",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#FDF2F0",
            terminal_fg: "#1A1210",
            terminal_cursor: "#E76F8B",
            terminal_selection: "rgba(231, 111, 139, 0.18)",
        },
        "cloud" => ThemeColors {
            bg: "#F4F6F8",
            bg_secondary: "#EAEDEF",
            bg_tertiary: "#E0E4E6",
            border: "#C8CED3",
            text: "#111827",
            text_muted: "#4B5563",
            text_dim: "#9CA3AF",
            accent: "#2563EB",
            accent_hover: "#1D4ED8",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#F4F6F8",
            terminal_fg: "#111827",
            terminal_cursor: "#2563EB",
            terminal_selection: "rgba(37, 99, 235, 0.18)",
        },
        "lavender" => ThemeColors {
            bg: "#F5F3FF",
            bg_secondary: "#EDE9FE",
            bg_tertiary: "#E2DCFD",
            border: "#CDC1F4",
            text: "#1E1B4B",
            text_muted: "#5B559E",
            text_dim: "#8E83C0",
            accent: "#7C3AED",
            accent_hover: "#5B21B6",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#F5F3FF",
            terminal_fg: "#1E1B4B",
            terminal_cursor: "#7C3AED",
            terminal_selection: "rgba(124, 58, 237, 0.18)",
        },
        "mint" => ThemeColors {
            bg: "#F0FDF4",
            bg_secondary: "#E7F9ED",
            bg_tertiary: "#DBF5E4",
            border: "#BCDCCD",
            text: "#11291A",
            text_muted: "#36653E",
            text_dim: "#0A8A52",
            accent: "#059669",
            accent_hover: "#047857",
            success: "#16A34A",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#F0FDF4",
            terminal_fg: "#11291A",
            terminal_cursor: "#059669",
            terminal_selection: "rgba(5, 150, 105, 0.18)",
        },
        "coral" => ThemeColors {
            bg: "#FFF5F5",
            bg_secondary: "#FFEBEA",
            bg_tertiary: "#FFE5E2",
            border: "#D5A8A2",
            text: "#1A0D0A",
            text_muted: "#7E544C",
            text_dim: "#A88278",
            accent: "#F97316",
            accent_hover: "#EA580C",
            success: "#059669",
            error: "#DC2626",
            warning: "#D97706",
            terminal_bg: "#FFF5F5",
            terminal_fg: "#1A0D0A",
            terminal_cursor: "#F97316",
            terminal_selection: "rgba(249, 115, 22, 0.18)",
        },
        _ => get_theme("onyx"),
    }
}

pub const ALL_THEMES: &[(&str, &str)] = &[
    ("onyx", "Onyx"),
    ("sage", "Sage"),
    ("volcanic", "Volcanic"),
    ("tidal", "Tidal"),
    ("obsidian", "Obsidian"),
    ("grove", "Grove"),
    ("midnight", "Midnight"),
    ("ember", "Ember"),
    ("ivory", "Ivory"),
    ("sand", "Sand"),
    ("ash", "Ash"),
    ("rose", "Rose"),
    ("cloud", "Cloud"),
    ("lavender", "Lavender"),
    ("mint", "Mint"),
    ("coral", "Coral"),
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

fn get_theme_extras(name: &str) -> ThemeExtras {
    match name {
        "onyx" => ThemeExtras {
            bg_atmosphere: "radial-gradient(circle at 12% 8%, rgba(96,165,250,0.14), transparent 28%), radial-gradient(circle at 84% 88%, rgba(147,197,253,0.08), transparent 30%), #0D0D0E",
            glow_opacity: "0.72",
            noise_opacity: "0.10",
            glow_color: "rgba(96, 165, 250, 0.28)",
        },
        "volcanic" => ThemeExtras {
            bg_atmosphere: "radial-gradient(circle at 14% 10%, rgba(230,106,60,0.16), transparent 30%), radial-gradient(circle at 80% 82%, rgba(244,155,110,0.08), transparent 32%), #14100F",
            glow_opacity: "0.76",
            noise_opacity: "0.12",
            glow_color: "rgba(230, 106, 60, 0.30)",
        },
        "tidal" => ThemeExtras {
            bg_atmosphere: "radial-gradient(circle at 16% 12%, rgba(45,212,191,0.14), transparent 30%), radial-gradient(circle at 82% 78%, rgba(94,237,216,0.08), transparent 32%), #0C1218",
            glow_opacity: "0.80",
            noise_opacity: "0.10",
            glow_color: "rgba(45, 212, 191, 0.28)",
        },
        _ => ThemeExtras {
            bg_atmosphere: "var(--bg)",
            glow_opacity: "0",
            noise_opacity: "0",
            glow_color: "transparent",
        },
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
    let extras = get_theme_extras(theme_name);
    set_data_theme(theme_name);
    set_css_property("--bgAtmosphere", extras.bg_atmosphere);
    set_css_property("--themeGlowOpacity", extras.glow_opacity);
    set_css_property("--themeNoiseOpacity", extras.noise_opacity);
    set_css_property("--themeGlowColor", extras.glow_color);
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
    set_css_property("--fontSize", &format!("{}", font_size));
}
