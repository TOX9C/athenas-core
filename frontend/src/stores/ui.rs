use dioxus::prelude::*;

// Re-export UITheme from types module (single source of truth)
pub use crate::types::theme::UITheme;

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
    /// Whether idle Shell panes should get an auto-generated random name
    /// and agent panes should surface their scraped task title. Toggleable
    /// from the General settings tab; persisted under `"auto_generate_titles"`.
    pub auto_generate_titles: bool,
    /// Whether to send the first prompt to the configured LLM to get a
    /// 2-3 word summary of what the agent is doing. Toggleable from the
    /// General settings tab; persisted under `"summarize_agent_titles"`.
    pub summarize_agent_titles: bool,
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
            auto_generate_titles: true,
            summarize_agent_titles: false,
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
