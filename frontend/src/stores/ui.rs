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
    /// Whether to auto-generate pane titles (idle shells) and summarize
    /// agent titles via LLM. Toggleable from the General settings tab;
    /// persisted under `"smart_pane_titles"`.
    pub smart_pane_titles: bool,
    /// When the user launches a swarm from SwarmModal, this carries the
    /// goal text into the NewSpaceModal so it isn't lost on the handoff.
    pub pending_swarm_goal: Option<String>,
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
            smart_pane_titles: true,
            pending_swarm_goal: None,
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
