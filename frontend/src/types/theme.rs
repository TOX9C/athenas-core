use dioxus::prelude::*;

/// Mythology-themed palettes. Dark: Nyx (obsidian+gold), Aegis (Aegean blue+bronze),
/// Erebus (true black+gold-leaf). Light: Pentelic (marble), Olive (parchment), Sky (cool marble).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UITheme {
    #[default]
    Nyx,
    Aegis,
    Erebus,
    Pentelic,
    Olive,
    Sky,
    System,
}

impl UITheme {
    pub fn is_dark(&self) -> bool {
        matches!(self, UITheme::Nyx | UITheme::Aegis | UITheme::Erebus)
    }

    /// Lowercase id — used as the persisted store key and palette lookup key.
    pub fn name(&self) -> &'static str {
        match self {
            UITheme::Nyx => "nyx",
            UITheme::Aegis => "aegis",
            UITheme::Erebus => "erebus",
            UITheme::Pentelic => "pentelic",
            UITheme::Olive => "olive",
            UITheme::Sky => "sky",
            UITheme::System => "system",
        }
    }

    /// Display label (capitalized) for UI surfaces.
    pub fn label(&self) -> &'static str {
        match self {
            UITheme::Nyx => "Nyx",
            UITheme::Aegis => "Aegis",
            UITheme::Erebus => "Erebus",
            UITheme::Pentelic => "Pentelic",
            UITheme::Olive => "Olive",
            UITheme::Sky => "Sky",
            UITheme::System => "System",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "nyx" => UITheme::Nyx,
            "aegis" => UITheme::Aegis,
            "erebus" => UITheme::Erebus,
            "pentelic" => UITheme::Pentelic,
            "olive" => UITheme::Olive,
            "sky" => UITheme::Sky,
            "system" => UITheme::System,
            _ => UITheme::Nyx,
        }
    }

    pub fn all() -> &'static [UITheme] {
        &[
            UITheme::Nyx,
            UITheme::Aegis,
            UITheme::Erebus,
            UITheme::Pentelic,
            UITheme::Olive,
            UITheme::Sky,
            UITheme::System,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Panel {
    Chat,
    #[default]
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
    pub custom_agents: Vec<crate::types::workspace::CustomAgent>,
}

impl Default for UIState {
    fn default() -> Self {
        Self {
            panel: Panel::default(),
            sidebar_visible: true,
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
            font_family: String::from("Monaspace Neon"),
            font_size: 14,
            custom_agents: Vec::new(),
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
