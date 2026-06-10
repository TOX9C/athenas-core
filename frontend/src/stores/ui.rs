use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UITheme {
    #[default]
    Onyx,
    Sage,
    Volcanic,
    Tidal,
    Obsidian,
    Grove,
    Midnight,
    Ember,
    Ivory,
    Sand,
    Ash,
    Rose,
    Cloud,
    Lavender,
    Mint,
    Coral,
    System,
}

impl UITheme {
    pub fn is_dark(&self) -> bool {
        !matches!(
            self,
            UITheme::Ivory
                | UITheme::Sand
                | UITheme::Ash
                | UITheme::Rose
                | UITheme::Cloud
                | UITheme::Lavender
                | UITheme::Mint
                | UITheme::Coral
                | UITheme::System
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            UITheme::Onyx => "onyx",
            UITheme::Sage => "sage",
            UITheme::Volcanic => "volcanic",
            UITheme::Tidal => "tidal",
            UITheme::Obsidian => "obsidian",
            UITheme::Grove => "grove",
            UITheme::Midnight => "midnight",
            UITheme::Ember => "ember",
            UITheme::Ivory => "ivory",
            UITheme::Sand => "sand",
            UITheme::Ash => "ash",
            UITheme::Rose => "rose",
            UITheme::Cloud => "cloud",
            UITheme::Lavender => "lavender",
            UITheme::Mint => "mint",
            UITheme::Coral => "coral",
            UITheme::System => "system",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "onyx" => UITheme::Onyx,
            "sage" => UITheme::Sage,
            "volcanic" => UITheme::Volcanic,
            "tidal" => UITheme::Tidal,
            "obsidian" => UITheme::Obsidian,
            "grove" => UITheme::Grove,
            "midnight" => UITheme::Midnight,
            "ember" => UITheme::Ember,
            "ivory" => UITheme::Ivory,
            "sand" => UITheme::Sand,
            "ash" => UITheme::Ash,
            "rose" => UITheme::Rose,
            "cloud" => UITheme::Cloud,
            "lavender" => UITheme::Lavender,
            "mint" => UITheme::Mint,
            "coral" => UITheme::Coral,
            "system" => UITheme::System,
            _ => UITheme::Onyx,
        }
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
            font_family: String::from("JetBrains Mono"),
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
