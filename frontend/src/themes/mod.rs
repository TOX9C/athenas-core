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

pub fn get_theme(name: &str) -> ThemeColors {
    match name {
        "void" => ThemeColors {
            bg: "#0a0a0a", bg_secondary: "#111111", bg_tertiary: "#1a1a1a", border: "#27272a",
            text: "#e4e4e7", text_muted: "#a1a1aa", text_dim: "#71717a",
            accent: "#38bdf8", accent_hover: "#7dd3fc",
            success: "#22c55e", error: "#ef4444", warning: "#f59e0b",
            terminal_bg: "#0a0a0a", terminal_fg: "#e4e4e7", terminal_cursor: "#38bdf8",
            terminal_selection: "rgba(56, 189, 248, 0.3)",
        },
        "ghost" => ThemeColors {
            bg: "#111118", bg_secondary: "#1a1a24", bg_tertiary: "#252530", border: "#32323e",
            text: "#e0dfe8", text_muted: "#9695a8", text_dim: "#5c5b6e",
            accent: "#38bdf8", accent_hover: "#7dd3fc",
            success: "#4ade80", error: "#fb7185", warning: "#fbbf24",
            terminal_bg: "#111118", terminal_fg: "#e0dfe8", terminal_cursor: "#38bdf8",
            terminal_selection: "rgba(56, 189, 248, 0.3)",
        },
        "plasma" => ThemeColors {
            bg: "#0d0d1a", bg_secondary: "#161626", bg_tertiary: "#202035", border: "#2d2d45",
            text: "#ddddf0", text_muted: "#9494b8", text_dim: "#5a5a7a",
            accent: "#2dd4bf", accent_hover: "#5eead4",
            success: "#34d399", error: "#f87171", warning: "#fcd34d",
            terminal_bg: "#0d0d1a", terminal_fg: "#ddddf0", terminal_cursor: "#2dd4bf",
            terminal_selection: "rgba(45, 212, 191, 0.3)",
        },
        "carbon" => ThemeColors {
            bg: "#121212", bg_secondary: "#1a1a1a", bg_tertiary: "#242424", border: "#333333",
            text: "#d4d4d8", text_muted: "#a1a1aa", text_dim: "#6b6b73",
            accent: "#94a3b8", accent_hover: "#cbd5e1",
            success: "#22c55e", error: "#ef4444", warning: "#f59e0b",
            terminal_bg: "#121212", terminal_fg: "#d4d4d8", terminal_cursor: "#94a3b8",
            terminal_selection: "rgba(148, 163, 184, 0.25)",
        },
        "hex" => ThemeColors {
            bg: "#0f1117", bg_secondary: "#171b24", bg_tertiary: "#212733", border: "#2e3648",
            text: "#d6e0f0", text_muted: "#8b99b0", text_dim: "#566174",
            accent: "#22d3ee", accent_hover: "#67e8f9",
            success: "#4ade80", error: "#f87171", warning: "#fbbf24",
            terminal_bg: "#0f1117", terminal_fg: "#d6e0f0", terminal_cursor: "#22d3ee",
            terminal_selection: "rgba(34, 211, 238, 0.25)",
        },
        "neon-tokyo" => ThemeColors {
            bg: "#0d0f1c", bg_secondary: "#151829", bg_tertiary: "#1f2238", border: "#2c304a",
            text: "#e8e0f0", text_muted: "#a898c0", text_dim: "#6a5e80",
            accent: "#fb7185", accent_hover: "#fda4af",
            success: "#34d399", error: "#fb7185", warning: "#fde68a",
            terminal_bg: "#0d0f1c", terminal_fg: "#e8e0f0", terminal_cursor: "#fb7185",
            terminal_selection: "rgba(251, 113, 133, 0.25)",
        },
        "obsidian" => ThemeColors {
            bg: "#13131a", bg_secondary: "#1c1c26", bg_tertiary: "#272733", border: "#353542",
            text: "#e4e2ea", text_muted: "#9e9bac", text_dim: "#626072",
            accent: "#fb923c", accent_hover: "#fdba74",
            success: "#4ade80", error: "#f87171", warning: "#fcd34d",
            terminal_bg: "#13131a", terminal_fg: "#e4e2ea", terminal_cursor: "#fb923c",
            terminal_selection: "rgba(251, 146, 60, 0.25)",
        },
        "nebula" => ThemeColors {
            bg: "#0c0e1a", bg_secondary: "#141728", bg_tertiary: "#1e2138", border: "#2b2f4a",
            text: "#dde0f2", text_muted: "#9498b8", text_dim: "#5a5e78",
            accent: "#38bdf8", accent_hover: "#7dd3fc",
            success: "#34d399", error: "#fb7185", warning: "#fbbf24",
            terminal_bg: "#0c0e1a", terminal_fg: "#dde0f2", terminal_cursor: "#38bdf8",
            terminal_selection: "rgba(56, 189, 248, 0.25)",
        },
        "storm" => ThemeColors {
            bg: "#0f1520", bg_secondary: "#171e2e", bg_tertiary: "#22293c", border: "#30394e",
            text: "#d4dce8", text_muted: "#8894a8", text_dim: "#556070",
            accent: "#38bdf8", accent_hover: "#7dd3fc",
            success: "#4ade80", error: "#f87171", warning: "#fbbf24",
            terminal_bg: "#0f1520", terminal_fg: "#d4dce8", terminal_cursor: "#38bdf8",
            terminal_selection: "rgba(56, 189, 248, 0.25)",
        },
        "infrared" => ThemeColors {
            bg: "#110a0a", bg_secondary: "#1c1212", bg_tertiary: "#281c1c", border: "#3a2828",
            text: "#f0dede", text_muted: "#b89090", text_dim: "#785858",
            accent: "#f87171", accent_hover: "#fca5a5",
            success: "#4ade80", error: "#fb7185", warning: "#fbbf24",
            terminal_bg: "#110a0a", terminal_fg: "#f0dede", terminal_cursor: "#f87171",
            terminal_selection: "rgba(248, 113, 113, 0.25)",
        },
        "nova" => ThemeColors {
            bg: "#0a0f14", bg_secondary: "#121a22", bg_tertiary: "#1c2630", border: "#2a3640",
            text: "#d4e8e0", text_muted: "#88a89c", text_dim: "#557066",
            accent: "#34d399", accent_hover: "#6ee7b7",
            success: "#22c55e", error: "#f87171", warning: "#fbbf24",
            terminal_bg: "#0a0f14", terminal_fg: "#d4e8e0", terminal_cursor: "#34d399",
            terminal_selection: "rgba(52, 211, 153, 0.25)",
        },
        "stealth" => ThemeColors {
            bg: "#101010", bg_secondary: "#181818", bg_tertiary: "#222222", border: "#303030",
            text: "#c8c8c8", text_muted: "#888888", text_dim: "#555555",
            accent: "#6b7280", accent_hover: "#9ca3af",
            success: "#22c55e", error: "#ef4444", warning: "#f59e0b",
            terminal_bg: "#101010", terminal_fg: "#c8c8c8", terminal_cursor: "#6b7280",
            terminal_selection: "rgba(107, 114, 128, 0.3)",
        },
        "hologram" => ThemeColors {
            bg: "#071a1a", bg_secondary: "#0f2626", bg_tertiary: "#183333", border: "#254444",
            text: "#c8f0f0", text_muted: "#7eb8b8", text_dim: "#4a7878",
            accent: "#2dd4bf", accent_hover: "#5eead4",
            success: "#4ade80", error: "#fb7185", warning: "#fbbf24",
            terminal_bg: "#071a1a", terminal_fg: "#c8f0f0", terminal_cursor: "#2dd4bf",
            terminal_selection: "rgba(45, 212, 191, 0.25)",
        },
        "dracula" => ThemeColors {
            bg: "#282a36", bg_secondary: "#323444", bg_tertiary: "#3c3f52", border: "#495068",
            text: "#f8f8f2", text_muted: "#bfbfb6", text_dim: "#5a7a9a",
            accent: "#50fa7b", accent_hover: "#8aff9e",
            success: "#50fa7b", error: "#ff5555", warning: "#f1fa8c",
            terminal_bg: "#282a36", terminal_fg: "#f8f8f2", terminal_cursor: "#50fa7b",
            terminal_selection: "rgba(80, 250, 123, 0.3)",
        },
        "athena" => ThemeColors {
            bg: "#0b0e13", bg_secondary: "#141820", bg_tertiary: "#1e232e", border: "#2a303e",
            text: "#e0e4ee", text_muted: "#8890a4", text_dim: "#525a6e",
            accent: "#38bdf8", accent_hover: "#7dd3fc",
            success: "#22c55e", error: "#ef4444", warning: "#f59e0b",
            terminal_bg: "#0b0e13", terminal_fg: "#e0e4ee", terminal_cursor: "#38bdf8",
            terminal_selection: "rgba(56, 189, 248, 0.3)",
        },
        "synthwave" => ThemeColors {
            bg: "#0a0f1a", bg_secondary: "#111828", bg_tertiary: "#1a2236", border: "#263044",
            text: "#e0e8f0", text_muted: "#90a0b8", text_dim: "#506878",
            accent: "#f92aad", accent_hover: "#ff6ec7",
            success: "#4ade80", error: "#ff5555", warning: "#fbbf24",
            terminal_bg: "#0a0f1a", terminal_fg: "#e0e8f0", terminal_cursor: "#f92aad",
            terminal_selection: "rgba(249, 42, 173, 0.3)",
        },
        "cybernetics" => ThemeColors {
            bg: "#080c14", bg_secondary: "#101820", bg_tertiary: "#1a242e", border: "#26323e",
            text: "#d0f0e0", text_muted: "#80b898", text_dim: "#4a7860",
            accent: "#00ff9f", accent_hover: "#66ffcc",
            success: "#22c55e", error: "#f87171", warning: "#fbbf24",
            terminal_bg: "#080c14", terminal_fg: "#d0f0e0", terminal_cursor: "#00ff9f",
            terminal_selection: "rgba(0, 255, 159, 0.2)",
        },
        "quantum" => ThemeColors {
            bg: "#090d16", bg_secondary: "#121824", bg_tertiary: "#1c2433", border: "#2a3448",
            text: "#d4e8f8", text_muted: "#88a8c8", text_dim: "#506878",
            accent: "#67e8f9", accent_hover: "#a5f3fc",
            success: "#34d399", error: "#fb7185", warning: "#fcd34d",
            terminal_bg: "#090d16", terminal_fg: "#d4e8f8", terminal_cursor: "#67e8f9",
            terminal_selection: "rgba(103, 232, 249, 0.2)",
        },
        "mecha" => ThemeColors {
            bg: "#0d1017", bg_secondary: "#161a24", bg_tertiary: "#212632", border: "#303744",
            text: "#e4e0d4", text_muted: "#a89c80", text_dim: "#686050",
            accent: "#fbbf24", accent_hover: "#fcd34d",
            success: "#4ade80", error: "#f87171", warning: "#f59e0b",
            terminal_bg: "#0d1017", terminal_fg: "#e4e0d4", terminal_cursor: "#fbbf24",
            terminal_selection: "rgba(251, 191, 36, 0.2)",
        },
        "abyss" => ThemeColors {
            bg: "#040408", bg_secondary: "#0c0c14", bg_tertiary: "#161620", border: "#222230",
            text: "#c8c8e0", text_muted: "#8080a0", text_dim: "#484860",
            accent: "#0ea5e9", accent_hover: "#38bdf8",
            success: "#22c55e", error: "#ef4444", warning: "#f59e0b",
            terminal_bg: "#040408", terminal_fg: "#c8c8e0", terminal_cursor: "#0ea5e9",
            terminal_selection: "rgba(14, 165, 233, 0.3)",
        },
        "paper" => ThemeColors {
            bg: "#fafafa", bg_secondary: "#f0f0f0", bg_tertiary: "#e4e4e7", border: "#d4d4d8",
            text: "#18181b", text_muted: "#52525b", text_dim: "#a1a1aa",
            accent: "#0284c7", accent_hover: "#0369a1",
            success: "#16a34a", error: "#dc2626", warning: "#d97706",
            terminal_bg: "#fafafa", terminal_fg: "#18181b", terminal_cursor: "#0284c7",
            terminal_selection: "rgba(2, 132, 199, 0.15)",
        },
        "chalk" => ThemeColors {
            bg: "#f5f5f0", bg_secondary: "#eaeae4", bg_tertiary: "#deded6", border: "#ccccc2",
            text: "#1a1a18", text_muted: "#555550", text_dim: "#999990",
            accent: "#0d9488", accent_hover: "#0f766e",
            success: "#16a34a", error: "#dc2626", warning: "#d97706",
            terminal_bg: "#f5f5f0", terminal_fg: "#1a1a18", terminal_cursor: "#0d9488",
            terminal_selection: "rgba(13, 148, 136, 0.15)",
        },
        "solar" => ThemeColors {
            bg: "#fdf6e3", bg_secondary: "#eee8d5", bg_tertiary: "#ddd6c1", border: "#c8c0ab",
            text: "#073642", text_muted: "#586e75", text_dim: "#93a1a1",
            accent: "#b58900", accent_hover: "#cb9a00",
            success: "#859900", error: "#dc322f", warning: "#cb4b16",
            terminal_bg: "#fdf6e3", terminal_fg: "#073642", terminal_cursor: "#b58900",
            terminal_selection: "rgba(181, 137, 0, 0.15)",
        },
        "arctic" => ThemeColors {
            bg: "#f0f4f8", bg_secondary: "#e2e8f0", bg_tertiary: "#cbd5e1", border: "#b0bec9",
            text: "#0f172a", text_muted: "#475569", text_dim: "#94a3b8",
            accent: "#0284c7", accent_hover: "#0369a1",
            success: "#16a34a", error: "#dc2626", warning: "#d97706",
            terminal_bg: "#f0f4f8", terminal_fg: "#0f172a", terminal_cursor: "#0284c7",
            terminal_selection: "rgba(2, 132, 199, 0.15)",
        },
        "ivory" => ThemeColors {
            bg: "#fffff0", bg_secondary: "#f5f5e0", bg_tertiary: "#e8e8d0", border: "#d4d4b8",
            text: "#1a1a0e", text_muted: "#555548", text_dim: "#999980",
            accent: "#b45309", accent_hover: "#92400e",
            success: "#16a34a", error: "#dc2626", warning: "#d97706",
            terminal_bg: "#fffff0", terminal_fg: "#1a1a0e", terminal_cursor: "#b45309",
            terminal_selection: "rgba(180, 83, 9, 0.15)",
        },
        _ => get_theme("void"),
    }
}

pub const ALL_THEMES: &[(&str, &str)] = &[
    ("void", "Void"),
    ("ghost", "Ghost"),
    ("plasma", "Plasma"),
    ("carbon", "Carbon"),
    ("hex", "Hex"),
    ("neon-tokyo", "Neon Tokyo"),
    ("obsidian", "Obsidian"),
    ("nebula", "Nebula"),
    ("storm", "Storm"),
    ("infrared", "Infrared"),
    ("nova", "Nova"),
    ("stealth", "Stealth"),
    ("hologram", "Hologram"),
    ("dracula", "Dracula"),
    ("athena", "Athena"),
    ("synthwave", "Synthwave"),
    ("cybernetics", "Cybernetics"),
    ("quantum", "Quantum"),
    ("mecha", "Mecha"),
    ("abyss", "Abyss"),
    ("paper", "Paper"),
    ("chalk", "Chalk"),
    ("solar", "Solar"),
    ("arctic", "Arctic"),
    ("ivory", "Ivory"),
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
    let safe_value = value.replace('\'', "\\'").replace('\"', "\\\"");
    let script = format!(
        "document.documentElement.style.setProperty('{}', '{}');",
        property, safe_value
    );
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Function::new_no_args(&script).call0(&window);
    }
}

fn set_data_theme(value: &str) {
    let safe_value = value.replace('\'', "\\'").replace('\"', "\\\"");
    let script = format!("document.documentElement.setAttribute('data-theme', '{}');", safe_value);
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
    set_css_property(
        "--fontFamily",
        &format!("'{}', monospace", font_family),
    );
    set_css_property("--fontSize", &format!("{}px", font_size));
}
