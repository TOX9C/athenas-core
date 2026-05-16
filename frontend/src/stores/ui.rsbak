use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UITheme {
    #[default]
    Void,
    Ghost,
    Plasma,
    Carbon,
    Hex,
    NeonTokyo,
    Obsidian,
    Nebula,
    Storm,
    Infrared,
    Nova,
    Stealth,
    Hologram,
    Dracula,
    Athena,
    Synthwave,
    Cybernetics,
    Quantum,
    Mecha,
    Abyss,
    Paper,
    Chalk,
    Solar,
    Arctic,
    Ivory,
    System,
}

impl UITheme {
    pub fn name(&self) -> &'static str {
        match self {
            UITheme::Void => "void",
            UITheme::Ghost => "ghost",
            UITheme::Plasma => "plasma",
            UITheme::Carbon => "carbon",
            UITheme::Hex => "hex",
            UITheme::NeonTokyo => "neon-tokyo",
            UITheme::Obsidian => "obsidian",
            UITheme::Nebula => "nebula",
            UITheme::Storm => "storm",
            UITheme::Infrared => "infrared",
            UITheme::Nova => "nova",
            UITheme::Stealth => "stealth",
            UITheme::Hologram => "hologram",
            UITheme::Dracula => "dracula",
            UITheme::Athena => "athena",
            UITheme::Synthwave => "synthwave",
            UITheme::Cybernetics => "cybernetics",
            UITheme::Quantum => "quantum",
            UITheme::Mecha => "mecha",
            UITheme::Abyss => "abyss",
            UITheme::Paper => "paper",
            UITheme::Chalk => "chalk",
            UITheme::Solar => "solar",
            UITheme::Arctic => "arctic",
            UITheme::Ivory => "ivory",
            UITheme::System => "system",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "void" => UITheme::Void,
            "ghost" => UITheme::Ghost,
            "plasma" => UITheme::Plasma,
            "carbon" => UITheme::Carbon,
            "hex" => UITheme::Hex,
            "neon-tokyo" => UITheme::NeonTokyo,
            "obsidian" => UITheme::Obsidian,
            "nebula" => UITheme::Nebula,
            "storm" => UITheme::Storm,
            "infrared" => UITheme::Infrared,
            "nova" => UITheme::Nova,
            "stealth" => UITheme::Stealth,
            "hologram" => UITheme::Hologram,
            "dracula" => UITheme::Dracula,
            "athena" => UITheme::Athena,
            "synthwave" => UITheme::Synthwave,
            "cybernetics" => UITheme::Cybernetics,
            "quantum" => UITheme::Quantum,
            "mecha" => UITheme::Mecha,
            "abyss" => UITheme::Abyss,
            "paper" => UITheme::Paper,
            "chalk" => UITheme::Chalk,
            "solar" => UITheme::Solar,
            "arctic" => UITheme::Arctic,
            "ivory" => UITheme::Ivory,
            "system" => UITheme::System,
            _ => UITheme::Void,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Panel {
    Chat,
    #[default]
    Terminal,
    Workspace,
    Editor,
    Settings,
    Browser,
    Kanban,
    Swarm,
    Plugin,
    Notifications,
    Agents,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SidebarSection {
    #[default]
    Spaces,
    Files,
    Agents,
    Plugins,
}

#[derive(Clone, PartialEq)]
pub struct UIState {
    pub panel: Panel,
    pub sidebar_visible: bool,
    pub sidebar_section: SidebarSection,
    pub sidebar_width: f64,
    pub theme: UITheme,
    pub is_mobile: bool,
    pub command_palette_open: bool,
    pub show_new_space_modal: bool,
    pub show_swarm_modal: bool,
    pub show_settings_modal: bool,
    pub fullscreen_pane_id: Option<String>,
    pub right_sidebar_open: bool,
    pub right_sidebar_tab: String,
    pub font_family: String,
    pub font_size: u8,
}

impl Default for UIState {
    fn default() -> Self {
        Self {
            panel: Panel::default(),
            sidebar_visible: false,
            sidebar_section: SidebarSection::default(),
            sidebar_width: 240.0,
            theme: UITheme::default(),
            is_mobile: false,
            command_palette_open: false,
            show_new_space_modal: false,
            show_swarm_modal: false,
            show_settings_modal: false,
            fullscreen_pane_id: None,
            right_sidebar_open: false,
            right_sidebar_tab: String::from("details"),
            font_family: String::from("JetBrains Mono"),
            font_size: 14,
        }
    }
}

/// Global UI store using Dioxus signals.
pub fn use_ui_store() -> Signal<UIState> {
    use_context::<Signal<UIState>>()
}

/// Initialize the UI store as a context provider.
pub fn provide_ui_store() {
    use_context_provider(|| Signal::new(UIState::default()));
}
