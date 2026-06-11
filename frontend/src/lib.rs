pub mod components;
pub mod stores;
pub mod tauri_bridge;
pub mod themes;
pub mod types;
pub mod utils;

use components::agents::agent_inspector::AgentInspector;
use components::agents::output_event_bus::OutputEventBus;
use components::athena::athena_panel::AthenaPanelMode;
use components::command_palette::CommandPalette;
use components::kanban::kanban_board::KanbanBoard;
use components::notifications::notification_bell::NotificationBell;
use components::notifications::notification_toast::NotificationToast;
use components::plugin::input_request_modal::InputRequestModal;
use components::plugin::plugin_event_bus::{provide_plugin_bus_store, PluginEventBus};
use components::right_sidebar::panel::RightSidebar;
use components::settings::settings_modal::SettingsModal;
use components::settings::SettingsPanel;
use components::shared::icon::{
    IconAgents, IconFiles, IconGrid, IconPlugins, IconPlus, IconSettings, IconSwarm, IconTerminal,
};
use components::shared::illustration::OwlMark;
use components::shared::toast::{provide_toast_store, ToastContainer};
use components::sidebar::Sidebar;
use components::swarm::swarm_board::SwarmBoard;
use components::swarm::swarm_modal::SwarmModal;
use components::workspace::new_space_modal::NewSpaceModal;
use components::workspace::terminal_grid::WorkspaceGrid;
use components::workspace::workspace_tabs::WorkspaceTabs;
use dioxus::prelude::*;
use stores::agent_output::provide_agent_output_store;
use stores::agent_status::provide_agent_status_store;
use stores::athena::{provide_athena_store, use_athena_store};
use stores::command::provide_command_store;
use stores::editor::provide_editor_store;
use stores::notification::provide_notification_store;
use stores::panel_manager::{provide_panel_manager_store, use_panel_manager_store, RightPanel};
use stores::session::provide_session_store;
use stores::swarm::provide_swarm_store;
use stores::task::provide_task_store;
use stores::terminal::{provide_terminal_store, use_terminal_store};
use stores::ui::{provide_ui_store, use_ui_store, Panel, SidebarSection};
use stores::workspace::{
    provide_workspace_store, use_workspace_store, AgentType, PaneConfig, Space, WorkspaceState,
};

/// Root application component — faithful port of App.tsx.
#[component]
pub fn App() -> Element {
    // Provide all 14 stores
    provide_ui_store();
    provide_workspace_store();
    provide_athena_store();
    provide_notification_store();
    provide_editor_store();
    provide_session_store();
    provide_swarm_store();
    provide_task_store();
    provide_command_store();
    provide_agent_output_store();
    provide_agent_status_store();
    provide_panel_manager_store();
    provide_toast_store();
    provide_plugin_bus_store();
    provide_terminal_store();

    let mut ui_state = use_ui_store();
    let workspace = use_workspace_store();
    let mut workspace_mut = use_workspace_store();
    let mut athena_state = use_athena_store();
    let mut panel_state = use_panel_manager_store();
    let mut terminal_store = use_terminal_store();

    // Track mounted spaces. The set is reconciled against the current
    // workspace state on every render so it stays bounded — entries for
    // spaces that no longer exist are dropped, preventing unbounded growth
    // across long sessions that create and destroy many workspaces.
    let mut mounted_spaces = use_signal(std::collections::HashSet::<String>::new);
    let mut platform = use_signal(|| {
        crate::utils::platform_utils::is_mac()
            .then_some("MacIntel")
            .unwrap_or("unknown")
            .to_string()
    });
    let mut is_maximized = use_signal(|| false);

    // ─── Resizable right sidebar state ─────────────────────────────────
    let mut right_sidebar_width = use_signal(|| 480i32);
    let mut rsb_drag_start_x = use_signal(|| 0i32);
    let mut rsb_drag_start_w = use_signal(|| 0i32);
    let mut rsb_is_dragging = use_signal(|| false);

    // Override platform with authoritative value from Tauri backend (navigator.userAgent is
    // unreliable in the release WKWebView — it strips "Mac" from the UA string).
    use_effect(move || {
        spawn(async move {
            if let Ok(os) = crate::tauri_bridge::window_platform().await {
                if os.contains("macos") || os.contains("mac") || os.contains("darwin") {
                    platform.set("MacIntel".to_string());
                } else {
                    platform.set(os);
                }
            }
        });
    });

    // Apply theme and font on mount (load persisted values from store)
    {
        let mut ui_state_for_load = ui_state.clone();
        use_effect(move || {
            let mut ui = ui_state_for_load.clone();
            spawn(async move {
                // Load theme from persist
                if let Ok(theme_name) = crate::tauri_bridge::store_get("theme").await {
                    if !theme_name.is_empty() {
                        let theme = crate::stores::ui::UITheme::from_name(&theme_name);
                        ui.write().theme = theme;
                        crate::themes::apply_theme_to_dom(&theme_name);
                    }
                }
                // Load font family from persist
                if let Ok(font_family) = crate::tauri_bridge::store_get("font_family").await {
                    if !font_family.is_empty() {
                        ui.write().font_family = font_family.clone();
                        crate::themes::apply_font_to_dom(&font_family, ui.read().font_size);
                    }
                }
                // Load font size from persist
                if let Ok(font_size_str) = crate::tauri_bridge::store_get("font_size").await {
                    if let Ok(size) = font_size_str.parse::<u8>() {
                        ui.write().font_size = size;
                        let fam = ui.read().font_family.clone();
                        crate::themes::apply_font_to_dom(&fam, size);
                    }
                }
                // Load custom agents from persist
                if let Ok(agents_json) = crate::tauri_bridge::store_get("custom_agents").await {
                    if !agents_json.is_empty() {
                        if let Ok(agents) = serde_json::from_str::<
                            Vec<crate::types::workspace::CustomAgent>,
                        >(&agents_json)
                        {
                            ui.write().custom_agents = agents;
                        }
                    }
                }
            });
        });
    }

    // Also apply current local settings synchronously on mount (in case store fetch is slow)
    {
        let theme_name = ui_state.read().theme.name().to_string();
        let font_family = ui_state.read().font_family.clone();
        let font_size = ui_state.read().font_size;
        use_effect(move || {
            crate::themes::apply_theme_to_dom(&theme_name);
            crate::themes::apply_font_to_dom(&font_family, font_size);
        });
    }

    // Restore workspaces from persistent store on startup
    {
        let mut ws = workspace.clone();
        use_effect(move || {
            spawn(async move {
                let loaded = WorkspaceState::load().await;
                *ws.write() = loaded;
            });
        });
        // Mark effect as run-once by not capturing any reactive dependencies
    }

    // Track mounted spaces — pruned each render to current space IDs so
    // removed spaces do not leak in the set indefinitely.
    let active_space_id = workspace.read().active_space_id.clone();
    if let Some(id) = &active_space_id {
        if !mounted_spaces.read().contains(id) {
            mounted_spaces.write().insert(id.clone());
        }
    }

    let spaces = workspace.read().spaces.clone();
    let current_space_ids: std::collections::HashSet<String> =
        spaces.iter().map(|s| s.id.clone()).collect();
    mounted_spaces.write().retain(|id| current_space_ids.contains(id));

    let active_space: Option<Space> = spaces
        .iter()
        .find(|s| Some(&s.id) == active_space_id.as_ref())
        .cloned();
    let mounted_space_ids = mounted_spaces.read().clone();
    let mounted_workspaces: Vec<Space> = spaces
        .iter()
        .filter(|space| {
            mounted_space_ids.contains(&space.id)
                || active_space_id.as_deref() == Some(space.id.as_str())
        })
        .cloned()
        .collect();

    let active_space_pane_ids: Vec<String> = active_space
        .as_ref()
        .map(|space| space.panes.iter().map(|pane| pane.id.clone()).collect())
        .unwrap_or_default();

    use_effect({
        let active_space_pane_ids = active_space_pane_ids.clone();
        move || {
            if active_space_pane_ids.is_empty() {
                return;
            }

            let current_active = terminal_store.read().active_session_id.clone();
            let is_current_visible = current_active
                .as_ref()
                .is_some_and(|id| active_space_pane_ids.iter().any(|pane_id| pane_id == id));

            if !is_current_visible {
                terminal_store
                    .write()
                    .set_active(active_space_pane_ids[0].clone());
            }
        }
    });

    let is_mac = platform().to_lowercase().contains("mac");
    let sidebar_open = ui_state.read().sidebar_visible;
    let active_panel = ui_state.read().panel;
    let theme_label = ui_state.read().theme.label().to_string();
    let right_sidebar_open = ui_state.read().right_sidebar_open;
    let main_flex_basis = if right_sidebar_open { "60%" } else { "100%" };

    // Pre-compute status bar strings (RSX can't handle complex expressions in interpolation)
    let status_workspace_name = active_space
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "No workspace".to_string());
    let status_pane_count = active_space
        .as_ref()
        .map(|s| format!("{} panes", s.panes.len()))
        .unwrap_or_default();
    let status_panel_str = match active_panel {
        Panel::Editor => "editor",
        Panel::Kanban => "kanban",
        Panel::Swarm => "swarm",
        Panel::Chat => "chat",
        Panel::Workspace => "workspace",
        Panel::Settings => "settings",
        Panel::Browser => "browser",
        Panel::Plugin => "plugin",
        Panel::Notifications => "notifications",
        Panel::Agents => "agents",
    }
    .to_string();

    rsx! {
        div {
            tabindex: "0",
            class: "app-root",
            style: "height: 100vh; width: 100vw; display: flex; flex-direction: column; overflow: hidden; background: var(--bg); outline: none;",

            // Global keybindings
            onkeydown: move |e: KeyboardEvent| {
                let mods = e.modifiers(); let meta = mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL);
                let shift = mods.contains(Modifiers::SHIFT);
                let key = e.key();
                if meta && !shift {
                    match key {
                        Key::Character(ref c) if c == "k" => {
                            let v = ui_state.read().command_palette_open; ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(ref c) if c == "j" => {
                            // Toggle right sidebar to Assistant (Athena) tab
                            let sidebar_open = ui_state.read().right_sidebar_open;
                            let active = panel_state.read().active_right_panel;
                            if !sidebar_open || active != RightPanel::Assistant {
                                panel_state.write().active_right_panel = RightPanel::Assistant;
                                ui_state.write().right_sidebar_open = true;
                            } else {
                                ui_state.write().right_sidebar_open = false;
                            }
                        }
                        Key::Character(ref c) if c == "t" => {
                            ui_state.write().show_new_space_modal = true;
                        }
                        Key::Character(ref c) if c == "b" => {
                            let v = ui_state.read().sidebar_visible; ui_state.write().sidebar_visible = !v;
                        }
                        Key::Character(ref c) if c == "1" => { ui_state.write().panel = Panel::Workspace; }
                        Key::Character(ref c) if c == "2" => { ui_state.write().panel = Panel::Editor; }
                        Key::Character(ref c) if c == "3" => { ui_state.write().panel = Panel::Kanban; }
                        Key::Character(ref c) if c == "4" => { ui_state.write().panel = Panel::Swarm; }
                    Key::Character(ref c) if c == "w" => {
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Some(active) = doc.active_element() {
                                    let tag = active.tag_name().to_lowercase();
                                    let is_editable = tag == "input" || tag == "textarea" ||
                                        active.get_attribute("contenteditable").is_some();
                                    if is_editable {
                                        return;
                                    }
                                }
                            }
                        }
                        let (space_id, first_pane_id) = {
                            let ws = workspace.read();
                            let sid = ws.active_space_id.clone();
                            let pane_id = sid.as_ref().and_then(|id| {
                                ws.spaces.iter()
                                    .find(|s| s.id == *id)
                                    .and_then(|s| s.panes.first().map(|p| p.id.clone()))
                            });
                            (sid, pane_id)
                        };
                        if let (Some(sid), Some(pid)) = (space_id, first_pane_id) {
                            workspace_mut.write().remove_pane_from_space(&sid, &pid);
                            e.prevent_default();
                        }
                    }
                        Key::Character(ref c) if c == "p" => {
                            let v = ui_state.read().command_palette_open; ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(ref c) if c == "e" => {
                            let current = ui_state.read().panel;
                            ui_state.write().panel = if current == Panel::Editor { Panel::Workspace } else { Panel::Editor };
                        }
                        Key::Character(ref c) if c == "," => {
                            ui_state.write().show_settings_modal = true;
                        }
                        Key::Character(ref c) if c == "\\" => {
                            let v = ui_state.read().right_sidebar_open; ui_state.write().right_sidebar_open = !v;
                        }
                        _ => {}
                    }
                }
                if meta && shift {
                    match key {
                        Key::Character(ref c) if c == "S" => { ui_state.write().show_swarm_modal = true; }
                        Key::Character(ref c) if c == "P" => {
                            let v = ui_state.read().command_palette_open; ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(ref c) if c == "A" => {
                            let active_id = {
                                let ws = workspace.read();
                                ws.active_space_id.clone()
                            };
                            if let Some(sid) = active_id {
                                let ts = js_sys::Date::now() as u64;
                                let pane = PaneConfig {
                                    id: format!("{:x}-sh", ts),
                                    agent_type: AgentType::Shell,
                                    custom_cmd: None,
                                    custom_agent_id: None,
                                    label: None,
                                    bypass_mode: None,
                                    project_name: None,
                                    model_name: None,
                                    resume_id: None,
                                };
                                workspace_mut.write().add_pane_to_space(&sid, pane);
                                e.prevent_default();
                            }
                        }
                        Key::Character(ref c) if c == "R" => {
                            ui_state.write().sidebar_width = 240.0;
                            ui_state.write().panel = Panel::Workspace;
                            ui_state.write().sidebar_section = SidebarSection::Spaces;
                        }
                        _ => {}
                    }
                }
                if key == Key::Escape {
                    if let Some(window) = web_sys::window() {
                        if let Some(doc) = window.document() {
                            if let Some(active) = doc.active_element() {
                                let tag = active.tag_name().to_lowercase();
                                let is_editable = tag == "input" || tag == "textarea" || active.get_attribute("contenteditable").is_some();
                                if is_editable {
                                    return;
                                }
                            }
                        }
                    }
                    let mut ui = ui_state.write();
                    ui.command_palette_open = false;
                    ui.show_new_space_modal = false;
                    ui.show_swarm_modal = false;
                    ui.show_settings_modal = false;
                    athena_state.write().is_open = false; // keep in sync for any legacy listeners
                    e.stop_propagation();
                }
            },

            // Title bar
            div {
                class: "titlebar reveal-1",
                style: "height: 38px; -webkit-app-region: drag; display: flex; align-items: center; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                // Mac spacer for traffic lights
                if is_mac {
                    div { style: "width: 80px; flex-shrink: 0;" }
                }

                // Workspace tabs (centered, flex-1)
                div { style: "flex: 1; display: flex; align-items: center; justify-content: center; gap: 4px; padding: 0 8px; min-width: 0;",
                    WorkspaceTabs { on_new_space: move |_| { ui_state.write().show_new_space_modal = true; } }
                }

                // Right toolbar buttons
                div { style: "display: flex; align-items: center; gap: 4px; padding-right: 14px; flex-shrink: 0; -webkit-app-region: no-drag;",

                    // Panel switcher (only when a workspace is active)
                    if active_space.is_some() {
                        div { style: "display: flex; align-items: center; margin-right: 4px;",
                            for (panel, label) in [
                                (Panel::Workspace, "workspace"),
                                (Panel::Kanban, "kanban"),
                                (Panel::Swarm, "swarm"),
                            ] {
                                {
                                    let is_active = active_panel == panel;
                                    let color = if is_active { "var(--accent)" } else { "var(--textDim)" };
                                    let weight = if is_active { "600" } else { "400" };
                                    rsx! {
                                        button {
                                            key: "{label}",
                                            style: "height: 28px; padding: 0 10px; border: none; background: transparent; color: {color}; cursor: pointer; font-size: 10px; font-weight: {weight};",
                                            onclick: move |_| ui_state.write().panel = panel,
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Add Shell pane
                    if active_space.is_some() {
                        button {
                            class: "icon-btn",
                            title: "Add Shell (Cmd+Shift+A)",
                            onclick: move |_| {
                                let active_id = {
                                    let ws = workspace.read();
                                    ws.active_space_id.clone()
                                };
                                if let Some(sid) = active_id {
                                    let ts = js_sys::Date::now() as u64;
                                    let pane = PaneConfig {
                                        id: format!("{:x}-sh", ts),
                                        agent_type: AgentType::Shell,
                                        custom_cmd: None,
                                        custom_agent_id: None,
                                        label: None,
                                        bypass_mode: None,
                                        project_name: None,
                                        model_name: None,
                                    resume_id: None,
                                    };
                                    workspace_mut.write().add_pane_to_space(&sid, pane);
                                }
                            },
                            IconPlus { size: Some(16), color: Some("currentColor".to_string()) }
                        }
                    }

                    // Athena toggle
                    button {
                        class: "icon-btn",
                        "data-athena-toggle": "",
                        title: "Athena (Cmd+J)",
                        onclick: move |_| {
                            let sidebar_open = ui_state.read().right_sidebar_open;
                            let active = panel_state.read().active_right_panel;
                            if !sidebar_open || active != RightPanel::Assistant {
                                panel_state.write().active_right_panel = RightPanel::Assistant;
                                ui_state.write().right_sidebar_open = true;
                            } else {
                                ui_state.write().right_sidebar_open = false;
                            }
                        },
                        IconTerminal { size: Some(16), color: Some("currentColor".to_string()) }
                    }

                    // Swarm launch
                    button {
                        class: "icon-btn",
                        title: "Launch Swarm",
                        onclick: move |_| { ui_state.write().show_swarm_modal = true; },
                        IconSwarm { size: Some(16), color: Some("currentColor".to_string()) }
                    }

                    // Notification bell
                    NotificationBell {}

                    // Settings
                    button {
                        class: "icon-btn",
                        title: "Settings (Cmd+,)",
                        onclick: move |_| { ui_state.write().show_settings_modal = true; },
                        IconSettings { size: Some(16), color: Some("currentColor".to_string()) }
                    }
                }

                // Non-Mac: window controls
                if !is_mac {
                    div { style: "display: flex; align-items: center; flex-shrink: 0; -webkit-app-region: no-drag;",
                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--textMuted); cursor: pointer;",
                            onclick: move |_| { spawn(async move { let _ = crate::tauri_bridge::window_minimize().await; }); },
                            "\u{2013}"
                        }
                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--textMuted); cursor: pointer;",
                            onclick: move |_| {
                                let maximized = is_maximized();
                                is_maximized.set(!maximized);
                                spawn(async move { let _ = crate::tauri_bridge::window_maximize().await; });
                            },
                            if is_maximized() { "\u{29C9}" } else { "\u{25A1}" }
                        }
                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--textMuted); cursor: pointer;",
                            onmouseover: move |e| { let _ = e; },
                            onclick: move |_| { spawn(async move { let _ = crate::tauri_bridge::window_close().await; }); },
                            "\u{00D7}"
                        }
                    }
                }
            }

            // Main content area
            div {
                class: "reveal-2",
                style: "display: flex; flex-direction: row; flex: 1; overflow: hidden; min-height: 0; position: relative;",

                // Left sidebar or sidebar rail
                if sidebar_open {
                    Sidebar { on_new_space: move |_| { ui_state.write().show_new_space_modal = true; } }
                } else {
                    // SidebarRail — compact icon strip for collapsed state
                    div {
                        style: "width: 28px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; padding: 8px 0; gap: 8px; border-right: 1px solid var(--border); background: var(--bgSecondary);",

                        button {
                            style: "padding: 4px;  border: none; background: transparent; color: var(--textMuted); cursor: pointer;",
                            title: "Expand sidebar",
                            onclick: move |_| { ui_state.write().sidebar_visible = true; },
                            "\u{203A}"
                        }

                        for (sec, label) in [
                            (SidebarSection::Spaces, "SP"),
                            (SidebarSection::Files, "FL"),
                            (SidebarSection::Agents, "AG"),
                            (SidebarSection::Plugins, "PL"),
                        ] {
                            {
                                rsx! {
                                    button {
                                        key: "{label}",
                                        style: "padding: 4px;  border: none; background: transparent; color: var(--textDim); cursor: pointer;",
                                        title: match sec {
                                            SidebarSection::Spaces => "Spaces",
                                            SidebarSection::Files => "Files",
                                            SidebarSection::Agents => "Agents",
                                            SidebarSection::Plugins => "Plugins",
                                        },
                                        onclick: move |_| {
                                            ui_state.write().sidebar_section = sec;
                                            ui_state.write().sidebar_visible = true;
                                        },
                                        {match sec {
                                            SidebarSection::Spaces => rsx! { IconGrid { size: Some(16), color: Some("var(--textDim)".to_string()) } },
                                            SidebarSection::Files => rsx! { IconFiles { size: Some(16), color: Some("var(--textDim)".to_string()) } },
                                            SidebarSection::Agents => rsx! { IconAgents { size: Some(15), color: Some("var(--textDim)".to_string()) } },
                                            SidebarSection::Plugins => rsx! { IconPlugins { size: Some(16), color: Some("var(--textDim)".to_string()) } },
                                        }}
                                    }
                                }
                            }
                        }
                    }
                }

                // Center content
                div {
                    style: "flex: 1; display: flex; flex-direction: column; overflow: hidden; min-width: 0; min-height: 0;",

                    div {
                        style: "flex: 1; display: flex; min-height: 0; min-width: 0;",

                        // Main panel area
                        div {
                            style: "flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; flex-basis: {main_flex_basis};",

                            // Active panel or empty state
                            if active_space.is_none() {
                                // Branded welcome — the lamp glow on .app-root shows through here.
                                div {
                                    class: "animate-rise",
                                    style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 28px;",

                                    div {
                                        style: "display: flex; flex-direction: column; align-items: center; gap: 14px;",

                                        div { class: "lamp-glow", OwlMark { size: Some(64) } }

                                        div { style: "display: flex; flex-direction: column; align-items: center; gap: 6px;",
                                            h2 {
                                                style: "font-family: var(--font-display); font-size: 34px; font-weight: 600; margin: 0; color: var(--text); letter-spacing: 0.02em;",
                                                "Athena\u{2019}s Core"
                                            }
                                            p {
                                                style: "font-size: 14px; margin: 0; color: var(--textMuted);",
                                                "Summon a workspace to begin the work."
                                            }
                                        }
                                    }

                                    button {
                                        class: "btn-primary",
                                        onclick: move |_| {
                                            web_sys::console::log_1(&"[EmptyState] New Workspace clicked".into());
                                            ui_state.write().show_new_space_modal = true;
                                        },
                                        IconPlus { size: Some(15), color: Some("currentColor".to_string()) }
                                        "New Workspace"
                                    }
                                }
                            } else {
                                div {
                                    style: "flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0;",
                                    div {
                                        style: if active_panel == Panel::Workspace {
                                            "flex: 1; display: flex; min-width: 0; min-height: 0; position: relative;"
                                        } else {
                                            "display: none;"
                                        },

                                        for space in mounted_workspaces.iter() {
                                            div {
                                                key: "workspace-view-{space.id}",
                                                style: if active_space_id.as_deref() == Some(space.id.as_str()) {
                                                    "position: absolute; inset: 0; display: flex; min-width: 0; min-height: 0;"
                                                } else {
                                                    "position: absolute; inset: 0; display: none; min-width: 0; min-height: 0;"
                                                },
                                                WorkspaceGrid {
                                                    key: "workspace-grid-{space.id}",
                                                    active_space: Some(space.clone()),
                                                    active_space_id: active_space_id.clone(),
                                                }
                                            }
                                        }
                                    }
                                    div {
                                        style: if active_panel == Panel::Editor {
                                            "flex: 1; display: flex; min-width: 0; min-height: 0;"
                                        } else {
                                            "display: none;"
                                        },
                                        div { style: "flex: 1; overflow: hidden;", "Editor panel" }
                                    }
                                    div {
                                        style: if active_panel == Panel::Kanban {
                                            "flex: 1; display: flex; min-width: 0; min-height: 0;"
                                        } else {
                                            "display: none;"
                                        },
                                        KanbanBoard {}
                                    }
                                    div {
                                        style: if active_panel == Panel::Swarm {
                                            "flex: 1; display: flex; min-width: 0; min-height: 0;"
                                        } else {
                                            "display: none;"
                                        },
                                        SwarmBoard {}
                                    }
                                    div {
                                        style: if active_panel == Panel::Settings {
                                            "flex: 1; display: flex; min-width: 0; min-height: 0;"
                                        } else {
                                            "display: none;"
                                        },
                                        SettingsPanel {}
                                    }
                                }
                            }
                        }

                        // Right sidebar (browser/assistant/editor)
                        if ui_state.read().right_sidebar_open {
                            div {
                                class: if rsb_is_dragging() { "resize-handle-col resize-handle is-dragging" } else { "resize-handle-col resize-handle" },
                                style: "width: 1px; flex-shrink: 0; cursor: col-resize; position: relative; align-self: stretch;",
                                onmouseover: move |e| { let _ = e; },
                                onmousedown: move |e: MouseEvent| {
                                    e.prevent_default();
                                    let coords = e.data.client_coordinates();
                                    rsb_drag_start_x.set(coords.x as i32);
                                    rsb_drag_start_w.set(right_sidebar_width());
                                    rsb_is_dragging.set(true);
                                },
                                div { style: "position: absolute; top: 0; bottom: 0; left: -4px; width: 9px; background: transparent;" }
                            }
                            div {
                                style: "width: {right_sidebar_width}px; min-width: {right_sidebar_width}px; flex-shrink: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden;",
                                RightSidebar {}
                            }
                            // Global drag capture overlay (while resizing)
                            if rsb_is_dragging() {
                                div {
                                    style: "position: fixed; inset: 0; z-index: 9999; cursor: col-resize;",
                                    onmousemove: move |e: MouseEvent| {
                                        let coords = e.data.client_coordinates();
                                        let dx = coords.x as i32 - rsb_drag_start_x();
                                        let new_w = rsb_drag_start_w() - dx;
                                        let clamped = new_w.clamp(200, 900);
                                        right_sidebar_width.set(clamped);
                                    },
                                    onmouseup: move |_| {
                                        rsb_is_dragging.set(false);
                                    },
                                    onmouseleave: move |_| {
                                        rsb_is_dragging.set(false);
                                    },
                                }
                            }
                        }
                    }

                }

                // Athena is rendered inside the right sidebar when active

                // Agent inspector (absolute overlay)
                AgentInspector {}
            }

            // Status bar
            div {
                style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 0 14px; border-top: 1px solid var(--border); height: 24px; background: var(--bgSecondary); color: var(--textDim); font-size: var(--text-xs);",

                span { style: "color: var(--textMuted);", "{status_workspace_name}" }
                span { style: "color: var(--textDim); opacity: 0.5;", "\u{00B7}" }
                span { "{status_pane_count}" }
                span { style: "color: var(--textDim); opacity: 0.5;", "\u{00B7}" }
                span { "{status_panel_str}" }
                div { style: "flex: 1;" }
                span {
                    style: "display: inline-flex; align-items: center; gap: 5px; color: var(--accent); font-weight: 600;",
                    span { style: "width: 5px; height: 5px; border-radius: 50%; background: var(--accent);" }
                    "{theme_label}"
                }
            }

            CommandPalette {}

            if ui_state.read().show_new_space_modal {
                NewSpaceModal {
                    on_close: move |_| { ui_state.write().show_new_space_modal = false; },
                }
            }

            if ui_state.read().show_swarm_modal {
                SwarmModal {
                    on_close: move |_| { ui_state.write().show_swarm_modal = false; },
                }
            }

            if ui_state.read().show_settings_modal {
                SettingsModal {
                    on_close: move |_| { ui_state.write().show_settings_modal = false; },
                }
            }

            InputRequestModal {}
            ToastContainer {}
            NotificationToast {}
            PluginEventBus {}
            OutputEventBus {}

            // Terminal sessions are spawned lazily inside the TerminalPaneBody component

            // Hidden triggers for command palette integration
            button {
                "data-new-space-trigger": "",
                style: "display: none;",
                onclick: move |_| { ui_state.write().show_new_space_modal = true; },
            }
            button {
                "data-swarm-trigger": "",
                style: "display: none;",
                onclick: move |_| { ui_state.write().show_swarm_modal = true; },
            }
        }
    }
}
