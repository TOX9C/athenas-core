pub mod components;
pub mod stores;
pub mod tauri_bridge;
pub mod themes;
pub mod types;
pub mod utils;
pub mod xterm_interop;

use components::agents::agent_inspector::AgentInspector;
use components::agents::output_event_bus::OutputEventBus;
use components::athena::athena_panel::AthenaPanel;
use components::command_palette::CommandPalette;
use components::kanban::kanban_board::KanbanBoard;
use components::notifications::notification_bell::NotificationBell;
use components::notifications::notification_toast::NotificationToast;
use components::plugin::input_request_modal::InputRequestModal;
use components::plugin::plugin_event_bus::{PluginEventBus, provide_plugin_bus_store};
use components::right_sidebar::browser_panel::RightBrowserPanel;
use components::settings::settings_modal::SettingsModal;
use components::shared::toast::{provide_toast_store, ToastContainer};
use components::sidebar::Sidebar;
use components::swarm::swarm_board::SwarmBoard;
use components::swarm::swarm_modal::SwarmModal;
use components::terminal::terminal_grid::TerminalGrid;
use components::workspace::new_space_modal::NewSpaceModal;
use components::workspace::workspace_tabs::WorkspaceTabs;
use dioxus::prelude::*;
use stores::agent_output::provide_agent_output_store;
use stores::agent_status::provide_agent_status_store;
use stores::athena::{provide_athena_store, use_athena_store};
use stores::command::provide_command_store;
use stores::editor::provide_editor_store;
use stores::layout::provide_layout_store;
use stores::notification::provide_notification_store;
use stores::panel_manager::provide_panel_manager_store;
use stores::session::provide_session_store;
use stores::swarm::provide_swarm_store;
use stores::task::provide_task_store;
use stores::terminal::provide_terminal_store;
use stores::ui::{provide_ui_store, use_ui_store, Panel, SidebarSection};
use stores::workspace::{provide_workspace_store, use_workspace_store, Space};

/// Root application component — faithful port of App.tsx.
#[component]
pub fn App() -> Element {
    // Provide all 14 stores
    provide_ui_store();
    provide_workspace_store();
    provide_athena_store();
    provide_notification_store();
    provide_editor_store();
    provide_terminal_store();
    provide_layout_store();
    provide_session_store();
    provide_swarm_store();
    provide_task_store();
    provide_command_store();
    provide_agent_output_store();
    provide_agent_status_store();
    provide_panel_manager_store();
    provide_toast_store();
    provide_plugin_bus_store();

    let mut ui_state = use_ui_store();
    let workspace = use_workspace_store();
    let mut workspace_mut = use_workspace_store();
    let mut athena_state = use_athena_store();

    let mut mounted_spaces = use_signal(std::collections::HashSet::<String>::new);
    let platform = use_signal(|| {
        if crate::utils::platform_utils::is_mac() {
            "MacIntel".to_string()
        } else {
            "unknown".to_string()
        }
    });
    let mut is_maximized = use_signal(|| false);
    let mut right_sidebar_tab = use_signal(|| "details".to_string());

    // Apply theme and font on mount
    {
        let theme_name = ui_state.read().theme.name().to_string();
        let font_family = ui_state.read().font_family.clone();
        let font_size = ui_state.read().font_size;
        use_effect(move || {
            crate::themes::apply_theme_to_dom(&theme_name);
            crate::themes::apply_font_to_dom(&font_family, font_size);
        });
    }

    // Track mounted spaces
    let active_space_id = workspace.read().active_space_id.clone();
    if let Some(id) = &active_space_id {
        if !mounted_spaces.read().contains(id) {
            mounted_spaces.write().insert(id.clone());
        }
    }

    let active_space: Option<Space> = workspace
        .read()
        .spaces
        .iter()
        .find(|s| Some(&s.id) == active_space_id.as_ref())
        .cloned();

    let is_mac = platform().to_lowercase().contains("mac");
    let sidebar_open = ui_state.read().sidebar_visible;
    let active_panel = ui_state.read().panel;
    let theme_str = ui_state.read().theme.name().to_string();

    rsx! {
        div {
            style: "height: 100vh; width: 100vw; display: flex; flex-direction: column; overflow: hidden; background: var(--bg);",

            // Global keybindings
            onkeydown: move |e: KeyboardEvent| {
                let mods = e.modifiers(); let meta = mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL);
                let shift = mods.contains(Modifiers::SHIFT);
                let key = e.key();
                if meta && !shift {
                    match key {
                        Key::Character(c) if c == "k" => {
                            let v = ui_state.read().command_palette_open; ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(c) if c == "j" => {
                            let current = athena_state.read().is_open;
                            athena_state.write().is_open = !current;
                        }
                        Key::Character(c) if c == "t" => {
                            ui_state.write().show_new_space_modal = true;
                        }
                        Key::Character(c) if c == "b" => {
                            let v = ui_state.read().sidebar_visible; ui_state.write().sidebar_visible = !v;
                        }
                        Key::Character(c) if c == "1" => { ui_state.write().panel = Panel::Terminal; }
                        Key::Character(c) if c == "2" => { ui_state.write().panel = Panel::Editor; }
                        Key::Character(c) if c == "3" => { ui_state.write().panel = Panel::Kanban; }
                        Key::Character(c) if c == "4" => { ui_state.write().panel = Panel::Swarm; }
                        Key::Character(c) if c == "w" => {
                            let space_id = workspace.read().active_space_id.clone();
                            if let Some(sid) = space_id {
                                let pane = workspace.read().spaces.iter()
                                    .find(|s| s.id == sid)
                                    .and_then(|s| s.panes.first().cloned());
                                if let Some(p) = pane {
                                    workspace_mut.write().remove_pane_from_space(&sid, &p.id);
                                }
                            }
                        }
                        Key::Character(c) if c == "p" => {
                            let v = ui_state.read().command_palette_open; ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(c) if c == "e" => {
                            let current = ui_state.read().panel;
                            ui_state.write().panel = if current == Panel::Editor { Panel::Terminal } else { Panel::Editor };
                        }
                        Key::Character(c) if c == "," => {
                            ui_state.write().show_settings_modal = true;
                        }
                        Key::Character(c) if c == "\\" => {
                            let v = ui_state.read().right_sidebar_open;
                            ui_state.write().right_sidebar_open = !v;
                        }
                        _ => {}
                    }
                } else if meta && shift {
                    match key {
                        Key::Character(c) if c == "p" => {
                            let v = ui_state.read().command_palette_open; ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(c) if c == "s" => {
                            ui_state.write().show_settings_modal = true;
                        }
                        Key::Character(c) if c == "r" => {
                            ui_state.write().panel = Panel::Terminal;
                            ui_state.write().sidebar_visible = true;
                            ui_state.write().right_sidebar_open = false;
                        }
                        _ => {}
                    }
                } else {
                    match key {
                        Key::Escape => {
                            let mut ui = ui_state.write();
                            ui.command_palette_open = false;
                            ui.show_new_space_modal = false;
                            ui.show_swarm_modal = false;
                            ui.show_settings_modal = false;
                        }
                        _ => {}
                    }
                }
            },

            // Titlebar
            div {
                style: "display: flex; align-items: center; flex-shrink: 0; border-bottom: 1px solid var(--border); height: 38px; background: var(--bgSecondary);",

                // macOS traffic light spacer
                if is_mac {
                    div { style: "width: 72px; flex-shrink: 0;" }
                }

                // Windows/Linux logo
                if !is_mac {
                    div {
                        style: "display: flex; align-items: center; gap: 4px; padding: 0 12px; flex-shrink: 0;",
                        span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.1em; color: var(--accent);", "ATHENA" }
                    }
                }

                // Workspace tabs centered
                div {
                    style: "flex: 1; display: flex; align-items: center; justify-content: center; gap: 4px; padding: 0 8px; min-width: 0;",
                    WorkspaceTabs { on_new_space: move |_| ui_state.write().show_new_space_modal = true }
                }

                // Right-side controls
                div {
                    style: "display: flex; align-items: center; gap: 2px; padding-right: 8px; flex-shrink: 0;",

                    // Panel switcher (when workspace active)
                    if active_space.is_some() {
                        div {
                            style: "display: flex; align-items: center; gap: 2px; margin-right: 4px;",

                            for (panel_enum, label) in [
                                (Panel::Chat, "chat"),
                                (Panel::Terminal, "terminals"),
                                (Panel::Editor, "panels"),
                                (Panel::Kanban, "kanban"),
                                (Panel::Swarm, "swarm"),
                            ] {
                                {
                                    let is_active = active_panel == panel_enum;
                                    let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                                    let fg = if is_active { "var(--text)" } else { "var(--textDim)" };
                                    let btn_style = format!(
                                        "padding: 2px 8px; border-radius: 4px; border: none; font-size: 10px; font-weight: 500; cursor: pointer; background: {bg}; color: {fg}; text-transform: capitalize;"
                                    );
                                    rsx! {
                                        button {
                                            key: "{label}",
                                            style: "{btn_style}",
                                            onclick: move |_| ui_state.write().panel = panel_enum,
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Athena toggle
                    button {
                        style: "padding: 4px 8px; border-radius: 6px; border: none; background: transparent; cursor: pointer; font-size: 11px; font-weight: 600; color: var(--textMuted);",
                        title: "Athena (Cmd+J)",
                        onclick: move |_| {
                            let current = athena_state.read().is_open;
                            athena_state.write().is_open = !current;
                        },
                        "AI"
                    }

                    // Right sidebar toggle
                    button {
                        style: "padding: 4px 8px; border-radius: 6px; border: none; background: transparent; cursor: pointer; font-size: 11px; font-weight: 600; color: var(--textMuted);",
                        title: "Right Sidebar (Cmd+\\)",
                        onclick: move |_| {
                            let v = ui_state.read().right_sidebar_open;
                            ui_state.write().right_sidebar_open = !v;
                        },
                        "RS"
                    }

                    // Swarm launcher
                    button {
                        style: "padding: 4px 8px; border-radius: 6px; border: none; background: transparent; cursor: pointer; font-size: 11px; font-weight: 600; color: var(--textMuted);",
                        title: "Launch Swarm",
                        onclick: move |_| ui_state.write().show_swarm_modal = true,
                        "SW"
                    }

                    // Notification bell
                    NotificationBell {}

                    // Settings
                    button {
                        style: "padding: 4px 8px; border-radius: 6px; border: none; background: transparent; cursor: pointer; font-size: 11px; font-weight: 600; color: var(--textMuted);",
                        title: "Settings",
                        onclick: move |_| ui_state.write().show_settings_modal = true,
                        "SET"
                    }
                }

                // Windows window controls
                if !is_mac {
                    div {
                        style: "display: flex; align-items: center; flex-shrink: 0;",

                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer; font-size: 16px; color: var(--textMuted);",
                            onclick: move |_| {

                                spawn(async move {

                                    let _ = tauri_bridge::window_minimize().await;

                                });

                            },
                            "\u{2013}"
                        }

                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                            onclick: move |_| {

                                let next = !is_maximized();

                                is_maximized.set(next);

                                spawn(async move {

                                    let _ = tauri_bridge::window_maximize().await;

                                });

                            },
                            span {
                                style: "font-size: 12px; color: var(--textMuted);",
                                {if is_maximized() { "\u{29c9}" } else { "\u{25a1}" }}
                            }
                        }

                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer; transition: background 0.15s;",
                            onclick: move |_| {

                                spawn(async move {

                                    let _ = tauri_bridge::window_close().await;

                                });

                            },
                            span { style: "font-size: 14px; color: var(--textMuted);", "\u{2715}" }
                        }
                    }
                }
            }

            // Main content
            div {
                style: "display: flex; flex: 1; min-height: 0;",

                // Sidebar
                if sidebar_open {
                    Sidebar { on_new_space: move |_| ui_state.write().show_new_space_modal = true }
                } else {
                    SidebarRail {
                        on_expand: move |_| ui_state.write().sidebar_visible = true,
                    }
                }

                // Content area
                div {
                    style: "flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0;",

                    div {
                        style: "flex: 1; display: flex; min-height: 0;",

                        // Main panel area
                        div {
                            style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",

                            if active_space.is_none() {
                                EmptyState { on_new_space: move |_| ui_state.write().show_new_space_modal = true }
                            } else {
                                match active_panel {
                                    Panel::Chat => rsx! { AthenaPanel {} },
                                    Panel::Kanban => rsx! { KanbanBoard {} },
                                    Panel::Swarm => rsx! { SwarmBoard {} },
                                    Panel::Editor => rsx! {
                                        div {
                                            style: "flex: 1; height: 100%; width: 100%;",
                                            // EditorPanel will be rendered here
                                        }
                                    },
                                    Panel::Terminal | _ => rsx! {
                                        div {
                                            style: "flex: 1; height: 100%; width: 100%; min-height: 0; position: relative;",

                                            for space in workspace.read().spaces.iter() {
                                                {
                                                    let is_mounted = mounted_spaces.read().contains(&space.id);
                                                    let is_active = Some(&space.id) == active_space_id.as_ref();
                                                    if is_mounted && is_active {
                                                        rsx! {
                                                            div {
                                                                key: "{space.id}",
                                                                style: "position: absolute; inset: 0; display: flex;",
                                                                TerminalGrid {}
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
            }
        }
    }
}
                                    },
                                }
                            }
                        }
                    }

                    // Athena panel (split)
                    if athena_state.read().is_open {
                        AthenaPanel {}
                    }
                }

                // Right sidebar
                if ui_state.read().right_sidebar_open {
                    div {
                        style: "flex-shrink: 0; width: 320px; border-left: 1px solid var(--border); display: flex; flex-direction: column; background: var(--bgSecondary);",

                        // Tab bar
                        div {
                            style: "display: flex; border-bottom: 1px solid var(--border);",
                            for (tab, label) in [("details", "Details"), ("browser", "Browser"), ("output", "Output"), ("assistant", "Assistant")] {
                                {
                                    let tab_str = tab.to_string();
                                    let is_active = right_sidebar_tab() == tab_str;
                                    let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                                    let fg = if is_active { "var(--text)" } else { "var(--textDim)" };
                                    let tab_for_click = tab.to_string();
                                    rsx! {
                                        button {
                                            key: "{label}",
                                            style: "flex: 1; padding: 6px 8px; border: none; background: {bg}; color: {fg}; font-size: 10px; font-weight: 600; cursor: pointer; text-transform: uppercase;",
                                            onclick: move |_| right_sidebar_tab.set(tab_for_click.clone()),
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }

                        // Tab content
                        div { style: "flex: 1; min-height: 0; overflow: auto;",
                            match right_sidebar_tab().as_str() {
                                "details" => rsx! { AgentInspector {} },
                                "browser" => rsx! { RightBrowserPanel {} },
                                "output" => rsx! { OutputEventBus {} },
                                "assistant" => rsx! {
                                    div { style: "padding: 16px; color: var(--textDim); font-size: 12px;",
                                        "Assistant panel coming soon."
                                    }
                                },
                                _ => rsx! {},
                            }
                        }
                    }
                }
            }

            // Status bar
            div {
                style: "flex-shrink: 0; display: flex; align-items: center; padding: 0 12px; border-top: 1px solid var(--border); height: 22px; background: var(--bgSecondary); font-size: 11px; color: var(--textDim);",

                span { {active_space.as_ref().map_or("No workspace".to_string(), |s| s.name.clone())} }
                span { style: "margin: 0 8px;", "|" }
                span {
                    {active_space.as_ref().map_or(String::new(), |s| format!("{} panes", s.panes.len()))}
                }
                span { style: "margin: 0 8px;", "|" }
                span {
                    style: "text-transform: capitalize;",
                    {format!("{:?}", active_panel).to_lowercase()}
                }
                div { style: "flex: 1;" }
                span {
                    style: "text-transform: capitalize;",
                    "{theme_str}"
                }
            }

            // Modals & overlays
            CommandPalette {}

            if ui_state.read().show_new_space_modal {
                NewSpaceModal { on_close: move |_| ui_state.write().show_new_space_modal = false }
            }

            if ui_state.read().show_swarm_modal {
                SwarmModal { on_close: move |_| ui_state.write().show_swarm_modal = false }
            }

            if ui_state.read().show_settings_modal {
                SettingsModal { on_close: move |_| ui_state.write().show_settings_modal = false }
            }

            InputRequestModal {}
            ToastContainer {}
            NotificationToast {}
            PluginEventBus {}
        }
    }
}

// -- SidebarRail -----------------------------------------------------------
/// Collapsed sidebar rail with section shortcuts.
#[derive(Props, Clone, PartialEq)]
struct SidebarRailProps {
    on_expand: EventHandler<()>,
}

#[component]
fn SidebarRail(props: SidebarRailProps) -> Element {
    let mut ui_state = use_ui_store();

    let rail_btn = "padding: 4px; border-radius: 4px; border: none; background: transparent; cursor: pointer; font-size: 9px; font-weight: 600; color: var(--textDim); width: 28px; text-align: center; letter-spacing: 0.03em;";

    rsx! {
        div {
            style: "flex-shrink: 0; display: flex; flex-direction: column; align-items: center; padding: 8px 0; gap: 6px; border-right: 1px solid var(--border); width: 28px; background: var(--bgSecondary);",

            button {
                style: "{rail_btn}",
                title: "Expand sidebar",
                onclick: move |_| props.on_expand.call(()),
                "\u{203a}"
            }

            button {
                style: "{rail_btn}",
                title: "Spaces",
                onclick: move |_| {
                    ui_state.write().sidebar_section = SidebarSection::Spaces;
                    ui_state.write().sidebar_visible = true;
                },
                "SP"
            }

            button {
                style: "{rail_btn}",
                title: "Files",
                onclick: move |_| {
                    ui_state.write().sidebar_section = SidebarSection::Files;
                    ui_state.write().sidebar_visible = true;
                },
                "FL"
            }

            button {
                style: "{rail_btn}",
                title: "Agents",
                onclick: move |_| {
                    ui_state.write().sidebar_section = SidebarSection::Agents;
                    ui_state.write().sidebar_visible = true;
                },
                "AG"
            }

            button {
                style: "{rail_btn}",
                title: "Plugins",
                onclick: move |_| {
                    ui_state.write().sidebar_section = SidebarSection::Plugins;
                    ui_state.write().sidebar_visible = true;
                },
                "PL"
            }
        }
    }
}

// -- EmptyState ------------------------------------------------------------
/// Shown when no workspace is active.
#[derive(Props, Clone, PartialEq)]
struct EmptyStateProps {
    on_new_space: EventHandler<()>,
}

#[component]
fn EmptyState(props: EmptyStateProps) -> Element {
    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 24px; color: var(--textDim);",

            div {
                style: "display: flex; flex-direction: column; align-items: center; gap: 8px;",

                // Logo mark
                div {
                    style: "width: 56px; height: 56px; border-radius: 12px; background: linear-gradient(135deg, var(--accent) 0%, var(--accentHover) 100%); display: flex; align-items: center; justify-content: center; opacity: 0.6;",
                    span { style: "font-size: 22px; font-weight: 800; color: #0b0e13; letter-spacing: -0.02em;", "A" }
                }

                h2 {
                    style: "font-size: 18px; font-weight: 600; color: var(--textMuted); margin: 0;",
                    "Athena's Core"
                }

                p {
                    style: "font-size: 14px; margin: 0;",
                    "Create a workspace to get started"
                }
            }

            button {
                style: "display: flex; align-items: center; gap: 8px; padding: 8px 20px; border-radius: 6px; border: none; font-size: 14px; font-weight: 500; cursor: pointer; background: var(--accent); color: #0b0e13; transition: background 0.15s;",
                onclick: move |_| props.on_new_space.call(()),
                "+ New Workspace"
            }

            // Keyboard shortcuts hint
            div {
                style: "display: flex; gap: 16px; font-size: 11px; color: var(--textDim);",

                span { "Cmd+T New" }
                span { "Cmd+K Palette" }
                span { "Cmd+J Athena" }
            }
        }
    }
}
