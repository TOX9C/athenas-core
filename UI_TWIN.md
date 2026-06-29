[UI TWIN PACKAGE — START]

# ATHENA'S CORE — Complete UI Package for Designer Agent

> A Tauri 2 desktop app with Dioxus 0.7 WASM frontend.
> Theme: "Obsidian & Gold / The Athenaeum" — a dark temple at night, lit by the lamp of wisdom.

---

## 1. FILE STRUCTURE

### Frontend Source Tree
```
frontend/
├── index.html                          — Entry HTML with WASM loading, console capture, fetch interceptor
├── public/styles.css                   — Global design system CSS (fonts, tokens, reset, animations)
├── public/fonts/*.woff2                — Cormorant (display), Hanken Grotesk (UI), Monaspace Neon (mono)
├── src/main.rs                         — Entry: launches Dioxus app
├── src/lib.rs                          — ROOT APP COMPONENT (App) — entire app layout, keybinds, panels
├── src/tauri_bridge.rs                 — IPC bridge for all backend calls
├── src/stores/                         — Signal-based state management (14 stores)
│   ├── mod.rs
│   ├── ui.rs                            — UITheme, Panel, SidebarSection, UIState
│   ├── workspace.rs                     — WorkspaceState, PaneConfig, Space, AgentType
│   ├── editor.rs
│   ├── terminal.rs                      — Terminal store, cell model, session state
│   ├── terminal_blocks.rs
│   ├── notification.rs
│   ├── session.rs
│   ├── command.rs
│   ├── athena.rs
│   ├── panel_manager.rs
│   ├── agent_output.rs
│   ├── agent_status.rs
│   ├── swarm.rs
│   └── task.rs
├── src/themes/mod.rs                   — 7 themes (Nyx/Aegis/Erebus/Pentelic/Olive/Sky), theme application
├── src/types/
│   ├── mod.rs
│   ├── workspace.rs                     — AgentType, GridTemplate, PaneConfig, Space, CustomAgent
│   ├── notification.rs
│   ├── theme.rs                         — UITheme with is_dark, name, label, from_name
│   └── swarm.rs / plugin.rs
├── src/utils/
│   ├── mod.rs
│   ├── agent_commands.rs                — Agent labels, colors, resume commands logic
│   ├── assistant_health.rs, assistant_logger.rs, circuit_breaker.rs, command_parser.rs
│   ├── file_icons.rs, fuzzy_search.rs
│   ├── highlighter.rs                    — Syntax highlighting helpers
│   ├── image_utils.rs, notification_sound.rs, platform_utils.rs, resume_scanner.rs
│   └── agent_commands.rs
└── src/components/
    ├── mod.rs
    ├── agents/
    │   ├── mod.rs, agent_inspector.rs, agent_output_line.rs, agent_output_panel.rs,
    │   ├── agent_selector.rs, agent_status_bar.rs, output_event_bus.rs
    ├── athena/
    │   ├── mod.rs, ask_user_block.rs, athena_input.rs, athena_panel.rs, chat_message.rs,
    │   ├── content_block.rs, thinking.rs, plan_block.rs, eval_block.rs, session_list.rs
    ├── command_palette/
    │   ├── mod.rs, command_palette_inner.rs
    ├── kanban/
    │   ├── mod.rs, kanban_board.rs, kanban_card.rs, kanban_column.rs
    ├── notifications/
    │   ├── mod.rs, notification_bell.rs, notification_panel.rs, notification_toast.rs
    ├── plugin/
    │   ├── mod.rs, agent_status_list.rs, input_request_modal.rs, plugin_card.rs,
    │   ├── plugin_dashboard.rs, plugin_event_bus.rs
    ├── right_sidebar/
    │   ├── mod.rs, panel.rs (RightSidebar), browser_panel.rs, editor_panel.rs, skills_panel.rs
    ├── settings/
    │   ├── mod.rs, settings_modal.rs, theme_picker.rs, shortcuts_ref.rs
    ├── shared/
    │   ├── mod.rs, button.rs, icon.rs, modal.rs, badge.rs, context_menu.rs, toast.rs,
    │   ├── tooltip.rs, resizable_panel.rs, segmented.rs, illustration.rs, error_boundary.rs
    ├── sidebar_dir/
    │   ├── mod.rs, file_explorer.rs, file_tree.rs, file_tree_node.rs,
    │   ├── workspace_list.rs, agent_panel.rs
    ├── swarm/
    │   ├── mod.rs, activity_feed.rs, agent_card.rs, role_badge.rs,
    │   ├── swarm_board.rs, swarm_launcher.rs, swarm_modal.rs
    └── workspace/
        ├── mod.rs, grid_template.rs, new_space_modal.rs, terminal_grid.rs,
        ├── workspace_tab.rs, workspace_tabs.rs, xterm_mount.rs
```

### Backend Source Tree (key files only)
```
src-tauri/
├── Cargo.toml
└── src/
    ├── main.rs         — App entry, registers Tauri commands, wires AppState
    ├── state.rs        — AppState with Arc<Mutex<T>> services
    └── commands/mod.rs — All #[tauri::command] handlers
```

---

## 2. CRITICAL FILES — FULL CONTENT

### 2.1 MAIN ENTRY POINT: frontend/src/main.rs
```rust
use athena_frontend::App;

fn main() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"[BOOT] main.rs entry reached".into());
    dioxus::prelude::launch(App);
    web_sys::console::log_1(&"[BOOT] dioxus::launch returned".into());
}
```

### 2.2 ROOT APP: frontend/src/lib.rs — Full Content
```rust
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
use stores::command::{provide_command_store, use_command_store, CommandState};
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

#[component]
pub fn App() -> Element {
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

    let mut mounted_spaces = use_signal(std::collections::HashSet::<String>::new);
    let mut platform = use_signal(|| {
        crate::utils::platform_utils::is_mac()
            .then_some("MacIntel")
            .unwrap_or("unknown")
            .to_string()
    });
    let mut is_maximized = use_signal(|| false);

    let mut right_sidebar_width = use_signal(|| 480i32);
    let mut rsb_drag_start_x = use_signal(|| 0i32);
    let mut rsb_drag_start_w = use_signal(|| 0i32);
    let mut rsb_is_dragging = use_signal(|| false);

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

    use_effect(move || {
        spawn(async move {
            if let Ok(maximized) = crate::tauri_bridge::window_is_maximized().await {
                is_maximized.set(maximized);
            }
        });
    });

    use_effect(move || {
        spawn(async move {
            let _unlisten = crate::tauri_bridge::listen(
                "tauri://resize",
                move |_payload| {
                    spawn(async move {
                        if let Ok(maximized) =
                            crate::tauri_bridge::window_is_maximized().await
                        {
                            is_maximized.set(maximized);
                        }
                    });
                },
            );
        });
    });

    {
        let mut ui_state_for_load = ui_state.clone();
        use_effect(move || {
            let mut ui = ui_state_for_load.clone();
            spawn(async move {
                if let Ok(theme_name) = crate::tauri_bridge::store_get("theme").await {
                    if !theme_name.is_empty() {
                        let theme = crate::stores::ui::UITheme::from_name(&theme_name);
                        ui.write().theme = theme;
                        crate::themes::apply_theme_to_dom(&theme_name);
                    }
                }
                if let Ok(font_family) = crate::tauri_bridge::store_get("font_family").await {
                    if !font_family.is_empty() {
                        ui.write().font_family = font_family.clone();
                        crate::themes::apply_font_to_dom(&font_family, ui.read().font_size);
                    }
                }
                if let Ok(font_size_str) = crate::tauri_bridge::store_get("font_size").await {
                    if let Ok(size) = font_size_str.parse::<u8>() {
                        ui.write().font_size = size;
                        let fam = ui.read().font_family.clone();
                        crate::themes::apply_font_to_dom(&fam, size);
                    }
                }
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

    {
        let theme_name = ui_state.read().theme.name().to_string();
        let font_family = ui_state.read().font_family.clone();
        let font_size = ui_state.read().font_size;
        use_effect(move || {
            crate::themes::apply_theme_to_dom(&theme_name);
            crate::themes::apply_font_to_dom(&font_family, font_size);
        });
    }

    {
        let mut ws = workspace.clone();
        use_effect(move || {
            spawn(async move {
                let loaded = WorkspaceState::load().await;
                let mut ws = ws.write();
                if ws.spaces.is_empty() && ws.active_space_id.is_none() {
                    *ws = loaded;
                }
            });
        });
    }

    {
        let mut cmd = use_command_store();
        use_effect(move || {
            spawn(async move {
                let loaded = CommandState::load_recent().await;
                cmd.write().recent_ids = loaded;
            });
        });
    }

    {
        let final_ws = workspace.clone();
        use_effect(move || {
            let Some(window) = web_sys::window() else { return; };
            let ws_signal = final_ws.clone();
            let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                ws_signal.read().save();
            }) as Box<dyn FnMut()>);
            let _ = js_sys::Reflect::set(
                &window,
                &wasm_bindgen::JsValue::from_str("onbeforeunload"),
                closure.as_ref(),
            );
            closure.forget();
        });
    }

    let active_space_id = workspace.read().active_space_id.clone();
    let spaces = workspace.read().spaces.clone();

    use_effect({
        let mut mounted_spaces = mounted_spaces.clone();
        let workspace = workspace.clone();
        move || {
            let ws = workspace.read();
            let existing_ids: std::collections::HashSet<String> =
                ws.spaces.iter().map(|s| s.id.clone()).collect();
            let active = ws.active_space_id.clone();
            let mut mounted = mounted_spaces.write();
            if let Some(id) = &active {
                mounted.insert(id.clone());
            }
            mounted.retain(|id| existing_ids.contains(id));
        }
    });

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

            onkeydown: move |e: KeyboardEvent| {
                let mods = e.modifiers();
                let meta = mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL);
                let shift = mods.contains(Modifiers::SHIFT);
                let key = e.key();
                if meta && !shift {
                    match key {
                        Key::Character(ref c) if c == "k" => {
                            let v = ui_state.read().command_palette_open;
                            ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(ref c) if c == "j" => {
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
                            let v = ui_state.read().sidebar_visible;
                            ui_state.write().sidebar_visible = !v;
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
                                        if is_editable { return; }
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
                            let v = ui_state.read().command_palette_open;
                            ui_state.write().command_palette_open = !v;
                        }
                        Key::Character(ref c) if c == "e" => {
                            let current = ui_state.read().panel;
                            ui_state.write().panel = if current == Panel::Editor { Panel::Workspace } else { Panel::Editor };
                        }
                        Key::Character(ref c) if c == "," => {
                            ui_state.write().show_settings_modal = true;
                        }
                        Key::Character(ref c) if c == "\\" => {
                            let v = ui_state.read().right_sidebar_open;
                            ui_state.write().right_sidebar_open = !v;
                        }
                        _ => {}
                    }
                }
                if meta && shift {
                    match key {
                        Key::Character(ref c) if c == "S" => { ui_state.write().show_swarm_modal = true; }
                        Key::Character(ref c) if c == "P" => {
                            let v = ui_state.read().command_palette_open;
                            ui_state.write().command_palette_open = !v;
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
                                    resume_cmd: None,
                                    resume_dismissed: None,
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
                                if is_editable { return; }
                            }
                        }
                    }
                    let mut ui = ui_state.write();
                    ui.command_palette_open = false;
                    ui.show_new_space_modal =;
                    ui.show_swarm_modal = false;
                    ui.show_settings_modal = false;
                    athena_state.write().is_open = false;
                    e.stop_propagation();
                }
            },

            // Title bar
            div {
                class: "titlebar reveal-1",
                style: "height: 38px; -webkit-app-region: drag; display: flex; align-items: center; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                if is_mac {
                    div { style: "width: 80px; flex-shrink: 0;" }
                }

                div { style: "flex: 1; display: flex; align-items: center; justify-content: center; gap: 4px; padding: 0 8px; min-width: 0; overflow: hidden;",
                    WorkspaceTabs { on_new_space: move |_| { ui_state.write().show_new_space_modal = true; } }
                }

                div { style: "display: flex; align-items: center; gap: 4px; padding-right: 14px; flex-shrink: 0; -webkit-app-region: no-drag;",

                    if active_space.is_some() {
                        div { class: "tb-panel-switcher", style: "display: flex; align-items: center; margin-right: 4px;",
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

                    if active_space.is_some() {
                        button {
                            class: "icon-btn tb-extra-btn",
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
                                        resume_cmd: None,
                                        resume_dismissed: None,
                                    };
                                    workspace_mut.write().add_pane_to_space(&sid, pane);
                                }
                            },
                            IconPlus { size: Some(16), color: Some("currentColor".to_string()) }
                        }
                    }

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

                    button {
                        class: "icon-btn tb-extra-btn",
                        title: "Launch Swarm",
                        onclick: move |_| { ui_state.write().show_swarm_modal = true; },
                        IconSwarm { size: Some(16), color: Some("currentColor".to_string()) }
                    }

                    NotificationBell {}

                    button {
                        class: "icon-btn",
                        title: "Settings (Cmd+,)",
                        onclick: move |_| { ui_state.write().show_settings_modal = true; },
                        IconSettings { size: Some(16), color: Some("currentColor".to_string()) }
                    }
                }

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

            div {
                class: "reveal-2",
                style: "display: flex; flex-direction: row; flex: 1; overflow: hidden; min-height: 0; position: relative;",

                if sidebar_open {
                    Sidebar { on_new_space: move |_| { ui_state.write().show_new_space_modal = true; } }
                } else {
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

                div {
                    style: "flex: 1; display: flex; flex-direction: column; overflow: hidden; min-width: 0; min-height: 0;",

                    div {
                        style: "flex: 1; display: flex; min-height: 0; min-width: 0;",

                        div {
                            style: "flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; flex-basis: {main_flex_basis};",

                            if active_space.is_none() {
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
                                    ... (other panels as div { display: none } conditionals)
                                }
                            }
                        }

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
                                    onmouseup: move |_| { rsb_is_dragging.set(false); },
                                    onmouseleave: move |_| { rsb_is_dragging.set(false); },
                                }
                            }
                        }
                    }

                }

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
                NewSpaceModal { on_close: move |_| { ui_state.write().show_new_space_modal = false; } }
            }

            if ui_state.read().show_swarm_modal {
                SwarmModal { on_close: move |_| { ui_state.write().show_swarm_modal = false; } }
            }

            if ui_state.read().show_settings_modal {
                SettingsModal { on_close: move |_| { ui_state.write().show_settings_modal = false; } }
            }

            InputRequestModal {}
            ToastContainer {}
            NotificationToast {}
            PluginEventBus {}
            OutputEventBus {}
        }
    }
}
```

### 2.3 GLOBAL STYLES: frontend/public/styles.css — Full Content

```css
/* ============================================================================
   ATHENA'S CORE — Design System
   "Obsidian & Gold / The Athenaeum"
   ============================================================================ */

/* ── 1. Fonts ─────────────────────────────────────────────────────────────── */
@font-face { font-family:'Cormorant'; font-style:normal; font-weight:500; font-display:swap;
  src:url('./fonts/Cormorant-500.woff2') format('woff2'); }
@font-face { font-family:'Cormorant'; font-style:normal; font-weight:600; font-display:swap;
  src:url('./fonts/Cormorant-600.woff2') format('woff2'); }

@font-face { font-family:'Hanken Grotesk'; font-style:normal; font-weight:400; font-display:swap;
  src:url('./fonts/HankenGrotesk-400.woff2') format('woff2'); }
@font-face { font-family:'Hanken Grotesk'; font-style:normal; font-weight:500; font-display:swap;
  src:url('./fonts/HankenGrotesk-500.woff2') format('woff2'); }
@font-face { font-family:'Hanken Grotesk'; font-style:normal; font-weight:600; font-display:swap;
  src:url('./fonts/HankenGrotesk-600.woff2') format('woff2'); }
@font-face { font-family:'Hanken Grotesk'; font-style:normal; font-weight:700; font-display:swap;
  src:url('./fonts/HankenGrotesk-700.woff2') format('woff2'); }

@font-face { font-family:'Monaspace Neon'; font-style:normal; font-weight:400; font-display:swap;
  src:url('./fonts/MonaspaceNeon-400.woff2') format('woff2'); }
@font-face { font-family:'Monaspace Neon'; font-style:normal; font-weight:500; font-display:swap;
  src:url('./fonts/MonaspaceNeon-500.woff2') format('woff2'); }

*, *::before, *::after { box-sizing: border-box; }

/* ── 2. Tokens (defaults = "Nyx": obsidian + gold) ───────────────────────────
   The theme engine (themes/mod.rs) overrides the color + atmosphere tokens at
   runtime. These defaults render correctly before the WASM theme pass runs. */
:root {
  /* surfaces */
  --bg: #0E0E11;
  --bgSecondary: #16161A;
  --bgTertiary: #1E1E23;
  --bgHover: #26262C;
  --border: #2A2A31;
  --borderActive: #3A3A43;

  /* text */
  --text: #ECEAE3;
  --textMuted: #9A968C;
  --textDim: #6A675E;

  /* signature accent — aged bronze-gold */
  --accent: #C9A24B;
  --accentHover: #E0BC6A;
  --accentSubtle: rgba(201, 162, 75, 0.12);
  --accentTeal: #4FA39E;          /* Aegean secondary (info / links) */
  --goldLeaf: #E7CE8F;
  --ring: rgba(201, 162, 75, 0.55);

  /* semantic — tuned to the palette */
  --success: #7BAE5A;   /* olive */
  --error:   #C5654D;   /* terracotta */
  --warning: #D2973C;   /* ochre */
  --green:  #7BAE5A;
  --blue:   #6FA6C9;
  --orange: #D2973C;
  --red:    #C5654D;
  --purple: #A98BC9;
  --cyan:   #4FA39E;

  /* terminal */
  --terminalBg: #0E0E11;
  --terminalFg: #ECEAE3;
  --terminalCursor: #C9A24B;
  --terminalSelection: rgba(201, 162, 75, 0.22);

  /* type families */
  --font-display: 'Cormorant', 'Iowan Old Style', Georgia, serif;
  --font-ui: 'Hanken Grotesk', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  --fontFamily: 'Monaspace Neon', 'JetBrains Mono', ui-monospace, Menlo, monospace;
  --font-mono: var(--fontFamily);

  /* type scale */
  --text-2xs: 10px;
  --text-xs: 11px;
  --text-sm: 12px;
  --text-base: 13px;
  --text-md: 15px;
  --text-lg: 18px;
  --text-xl: 24px;
  --text-2xl: 32px;
  --fontSize: 13px;          /* legacy alias */
  --lh-tight: 1.25;
  --lh: 1.5;

  /* spacing scale (4-based) */
  --space-1: 4px;  --space-2: 8px;  --space-3: 12px; --space-4: 16px;
  --space-5: 20px; --space-6: 24px; --space-8: 32px;

  /* radius */
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;
  --radius-pill: 999px;

  /* elevation — soft, warm-black */
  --shadow-sm: 0 1px 2px rgba(0,0,0,0.30);
  --shadow-md: 0 6px 20px rgba(0,0,0,0.38);
  --shadow-lg: 0 20px 60px rgba(0,0,0,0.50);
  --inset-hairline: inset 0 0 0 1px var(--border);

  /* motion */
  --ease: cubic-bezier(0.22, 0.61, 0.36, 1);
  --dur-fast: 140ms;
  --dur: 200ms;
  --dur-slow: 320ms;

  /* atmosphere (theme-driven; defaults give Nyx a faint gold lamp glow) */
  --themeGlowColor: rgba(201, 162, 75, 0.10);
  --themeGlowOpacity: 1;
  --themeNoiseOpacity: 0.022;

  --scrollbar-width: 6px;
  --scrollbar-height: 6px;
}

/* ── 3. Base ─────────────────────────────────────────────────────────────── */
html, body, #main {
  height: 100%;
  width: 100%;
  margin: 0;
  padding: 0;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font-ui);
  font-size: var(--text-base);
  line-height: var(--lh);
  -webkit-font-smoothing: antialiased;
  text-rendering: optimizeLegibility;
}

/* monospace surfaces */
.terminal-pane, .editor-panel, .code-content, .terminal-container, .mono {
  font-family: var(--fontFamily);
}

/* Root app surface carries the lamp glow (shows through transparent regions,
   e.g. the welcome screen + panel gutters). */
.app-root {
  background:
    radial-gradient(135% 90% at 50% -12%, var(--themeGlowColor) 0%, transparent 60%),
    var(--bg) !important;
}

::-webkit-scrollbar { width: var(--scrollbar-width); height: var(--scrollbar-height); }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--bgTertiary); border-radius: var(--radius-pill); }
::-webkit-scrollbar-thumb:hover { background: var(--textDim); }

.terminal-body ::selection { background: inherit; color: inherit; }
::selection { background: var(--accentSubtle); color: var(--text); }

input, textarea, select { font-family: inherit; }
button { cursor: pointer; border: none; background: none; font-family: inherit; color: inherit; }

/* Token-driven focus ring (gold). */
:focus-visible {
  outline: 2px solid var(--ring);
  outline-offset: 1px;
}

h1, h2, h3, .display { font-family: var(--font-display); font-weight: 600; letter-spacing: 0.01em; }

/* ── 4. Atmosphere — grain overlay on top of everything (pointer-safe) ─────── */
body::after {
  content: "";
  position: fixed;
  inset: 0;
  z-index: 9000;
  pointer-events: none;
  opacity: var(--themeNoiseOpacity);
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
}

/* Greek-key (meander) hairline rule — opt-in decorative divider. */
.meander-rule {
  height: 6px;
  border: 0;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='6'%3E%3Cpath d='M0 5h4V1h4v4h4V1h4v4h4' fill='none' stroke='%23C9A24B' stroke-opacity='0.5' stroke-width='1'/%3E%3C/svg%3E");
  background-repeat: repeat-x;
  background-position: center;
  opacity: 0.5;
}

/* ── 5. Motion ───────────────────────────────────────────────────────────── */
@keyframes spin { to { transform: rotate(360deg); } }
@keyframes reveal-rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
@keyframes reveal-fade { from { opacity: 0; } to { opacity: 1; } }
@keyframes modal-rise { from { opacity: 0; transform: translateY(12px) scale(0.985); } to { opacity: 1; transform: none; } }
@keyframes scrim-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes toast-in { from { opacity: 0; transform: translateX(16px); } to { opacity: 1; transform: none; } }
@keyframes lamp-glow {
  0%, 100% { opacity: 0.55; filter: drop-shadow(0 0 2px var(--accentSubtle)); }
  50%      { opacity: 1;    filter: drop-shadow(0 0 8px var(--accent)); }
}
@keyframes pulse-soft { 0%,100% { opacity: 0.5; } 50% { opacity: 1; } }

.animate-rise { animation: reveal-rise var(--dur-slow) var(--ease) both; }
.animate-fade { animation: reveal-fade var(--dur-slow) var(--ease) both; }
.lamp-glow { animation: lamp-glow 2.4s ease-in-out infinite; }
.pulse-soft { animation: pulse-soft 1.6s ease-in-out infinite; }

.reveal-1 { animation: reveal-fade var(--dur-slow) var(--ease) both; }
.reveal-2 { animation: reveal-rise var(--dur-slow) var(--ease) 60ms both; }
.reveal-3 { animation: reveal-rise var(--dur-slow) var(--ease) 120ms both; }

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.001ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.001ms !important;
  }
}

/* ── 6. Syntax highlighting ──────────────────────────────────────────────── */
.token-keyword { color: var(--purple); }
.token-string { color: var(--green); }
.token-comment { color: var(--textDim); font-style: italic; }
.token-number { color: var(--orange); }
.token-type { color: var(--orange); }
.token-function { color: var(--blue); }
.token-operator { color: var(--cyan); }
.token-lifetime { color: var(--cyan); }

.code-line { display: block; padding: 0 var(--space-2); white-space: pre; font-family: var(--fontFamily); font-size: var(--text-sm); line-height: 1.6; }
.code-line:hover { background: rgba(255, 255, 255, 0.03); }
.line-number { display: inline-block; width: 3em; text-align: right; padding-right: 1em; color: var(--textDim); user-select: none; opacity: 0.5; }

/* ── SEMANTIC UTILITY CLASSES ───────────────────────────────────────────── */
.bg-base { background-color: var(--bg); }
.bg-secondary { background-color: var(--bgSecondary); }
.bg-tertiary { background-color: var(--bgTertiary); }
.bg-elevated { background-color: var(--bgTertiary); }
.bg-surface { background-color: var(--bgSecondary); }
.bg-accent { background-color: var(--accent); }
.bg-accent-subtle { background-color: var(--accentSubtle); }
.bg-hover { background-color: var(--bgHover); }
.bg-transparent { background-color: transparent; }

.text-primary { color: var(--text); }
.text-muted { color: var(--textMuted); }
.text-dim { color: var(--textDim); }
.text-accent { color: var(--accent); }
.text-accent-hover { color: var(--accentHover); }
.text-inverse { color: var(--bg); }
.text-success { color: var(--success); }
.text-error { color: var(--error); }
.text-warning { color: var(--warning); }

.border-subtle { border: 1px solid var(--border); }
.border-accent { border: 1px solid var(--accent); }
.border-transparent { border: 1px solid transparent; }

.font-display { font-family: var(--font-display); }
.font-ui { font-family: var(--font-ui); }
.font-mono { font-family: var(--fontFamily); }

.panel-base { background-color: var(--bg); border: 1px solid var(--border); }
.fill { width: 100%; height: 100%; }
.flex-col { display: flex; flex-direction: column; }
.flex-row { display: flex; flex-direction: row; }
.flex-center { display: flex; align-items: center; justify-content: center; }
.flex-between { display: flex; align-items: center; justify-content: space-between; }
.flex-grow { flex-grow: 1; }
.flex-shrink { flex-shrink: 0; }

.gap-2 { gap: 2px; } .gap-4 { gap: 4px; } .gap-8 { gap: 8px; }
.gap-12 { gap: 12px; } .gap-16 { gap: 16px; } .gap-24 { gap: 24px; }

.pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 9px;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  font-weight: 500;
  background-color: var(--bgTertiary);
  color: var(--textMuted);
  border: 1px solid var(--border);
}

.badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 1px 7px;
  border-radius: var(--radius-pill);
  font-size: var(--text-2xs);
  font-weight: 600;
  letter-spacing: 0.02em;
  background-color: var(--accentSubtle);
  color: var(--accent);
}

/* Buttons — token-driven, with hover/active/focus motion */
.btn-primary, .btn-ghost, .btn-secondary {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 16px;
  border-radius: var(--radius-md);
  font-size: var(--text-base);
  font-weight: 600;
  font-family: var(--font-ui);
  cursor: pointer;
  border: 1px solid transparent;
  transition: background-color var(--dur-fast) var(--ease),
              border-color var(--dur-fast) var(--ease),
              transform var(--dur-fast) var(--ease),
              color var(--dur-fast) var(--ease);
}
.btn-primary { background-color: var(--accent); color: #1A150A; }
.btn-primary:hover { background-color: var(--accentHover); }
.btn-primary:active { transform: translateY(1px); }

.btn-secondary { background-color: var(--bgTertiary); color: var(--text); border-color: var(--border); }
.btn-secondary:hover { background-color: var(--bgHover); border-color: var(--borderActive); }
.btn-secondary:active { transform: translateY(1px); }

.btn-ghost { background-color: transparent; color: var(--text); border-color: var(--border); }
.btn-ghost:hover { background-color: var(--bgHover); }
.btn-ghost:active { transform: translateY(1px); }

.btn-danger { background-color: transparent; color: var(--error); border-color: var(--border); }
.btn-danger:hover { background-color: color-mix(in srgb, var(--error) 14%, transparent); border-color: var(--error); }
.btn-danger:active { transform: translateY(1px); }

.btn-sm { padding: 5px 11px; font-size: var(--text-sm); }

.btn-primary:disabled, .btn-secondary:disabled, .btn-ghost:disabled,
.btn-danger:disabled, .icon-btn:disabled, button:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}
.btn-primary:disabled:hover { background: var(--accent); }
.btn-secondary:disabled:hover { background: var(--bgTertiary); border-color: var(--border); }
.btn-ghost:disabled:hover { background: transparent; }
.btn-primary:disabled:active, .btn-secondary:disabled:active,
.btn-ghost:disabled:active, .btn-danger:disabled:active { transform: none; }

/* Icon-only button */
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--textMuted);
  cursor: pointer;
  transition: background-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.icon-btn:hover { background: var(--bgHover); color: var(--text); }
.icon-btn:active { transform: translateY(1px); }
.icon-btn.is-active { color: var(--accent); background: var(--accentSubtle); }

/* Token-driven form field. */
.field {
  width: 100%;
  padding: 8px 11px;
  background: var(--bg);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  font-family: var(--font-ui);
  font-size: var(--text-base);
  transition: border-color var(--dur-fast) var(--ease), box-shadow var(--dur-fast) var(--ease);
  outline: none;
}
.field::placeholder { color: var(--textDim); }
.field:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accentSubtle); }

/* Card surface. */
.card {
  background: var(--bgSecondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  transition: border-color var(--dur-fast) var(--ease), background-color var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease);
}
.card.is-interactive { cursor: pointer; }
.card.is-interactive:hover { border-color: var(--borderActive); background: var(--bgTertiary); }

/* Sidebar / list rows. */
.workspace-row { transition: background-color var(--dur-fast) var(--ease); }
.workspace-row:hover { background: var(--bgHover); }

/* Tooltip bubble */
.tip-bubble {
  position: absolute;
  z-index: 9500;
  padding: 4px 9px;
  background: var(--bgTertiary);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-family: var(--font-ui);
  white-space: nowrap;
  box-shadow: var(--shadow-md);
  pointer-events: none;
  animation: reveal-fade var(--dur-fast) var(--ease) both;
}

/* Modal scrim + container motion. */
.modal-scrim { animation: scrim-in var(--dur) var(--ease) both; backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px); }
.modal-card { animation: modal-rise var(--dur) var(--ease) both; box-shadow: var(--shadow-lg); }
.toast-card { animation: toast-in var(--dur) var(--ease) both; box-shadow: var(--shadow-md); }

/* Resize handle */
.resize-handle { background: var(--border); }

/* Pane dividers */
.pane-divider-col, .pane-divider-row { background: transparent; }
.pane-divider-col::before {
  content: "";
  position: absolute;
  top: 0; bottom: 0; left: 50%;
  transform: translateX(-50%);
  width: 1px;
  background: var(--border);
}
.pane-divider-row::before {
  content: "";
  position: absolute;
  left: 0; right: 0; top: 50%;
  transform: translateY(-50%);
  height: 1px;
  background: var(--border);
}

/* Pane focus ring — the clicked terminal gets a subtle gold outline. */
.pane-focus-ring {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 4;
  box-shadow: inset 0 0 0 1.5px var(--accent);
}

/* Segmented control. */
.segmented {
  display: inline-flex;
  padding: 2px;
  gap: 2px;
  background: var(--bgTertiary);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}
.segmented-item {
  padding: 4px 12px;
  border: none;
  background: transparent;
  color: var(--textMuted);
  font-size: var(--text-sm);
  font-weight: 500;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.segmented-item:hover { color: var(--text); }
.segmented-item.is-active { background: var(--accent); color: var(--bg); }

/* Context menu. */
.context-menu {
  position: fixed;
  z-index: 9600;
  min-width: 160px;
  padding: 4px;
  background: var(--bgSecondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  animation: reveal-fade var(--dur-fast) var(--ease) both;
}
.context-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  background: transparent;
  color: var(--text);
  font-size: var(--text-base);
  text-align: left;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.context-menu-item:hover { background: var(--bgHover); }
.context-menu-item.is-danger { color: var(--error); }

/* Spacing utilities */
.p-4 { padding: 4px; } .p-8 { padding: 8px; } .p-12 { padding: 12px; }
.p-16 { padding: 16px; } .p-24 { padding: 24px; }
.px-8 { padding-left: 8px; padding-right: 8px; }
.px-12 { padding-left: 12px; padding-right: 12px; }
.px-16 { padding-left: 16px; padding-right: 16px; }
.py-8 { padding-top: 8px; padding-bottom: 8px; }
.py-12 { padding-top: 12px; padding-bottom: 12px; }
.py-16 { padding-top: 16px; padding-bottom: 16px; }
.m-4 { margin: 4px; } .m-8 { margin: 8px; }
.mx-8 { margin-left: 8px; margin-right: 8px; }
.mx-12 { margin-left: 12px; margin-right: 12px; }
.my-8 { margin-top: 8px; margin-bottom: 8px; }
.my-12 { margin-top: 12px; margin-bottom: 12px; }

.pointer { cursor: pointer; }
.overflow-hidden { overflow: hidden; }
.overflow-auto { overflow: auto; }
.overflow-y-auto { overflow-y: auto; }
.overflow-x-auto { overflow-x: auto; }
.relative { position: relative; }
.absolute { position: absolute; }
.sticky { position: sticky; }

.rounded-sm { border-radius: var(--radius-sm); }
.rounded-md { border-radius: var(--radius-md); }
.rounded-lg { border-radius: var(--radius-lg); }
.rounded-pill { border-radius: var(--radius-pill); }

/* ── Responsive titlebar ─────────────────────────────────────────────────── */
@media (max-width: 680px) {
  .tb-panel-switcher { display: none !important; }
}
@media (max-width: 520px) {
  .tb-extra-btn { display: none !important; }
}

/* ── XTERM.JS overrides ──────────────────────────────────────────────────── */
.xterm .xterm-viewport { background-color: var(--bg) !important; }
.xterm-mount { contain: layout paint; }
.xterm-mount .xterm { overflow: hidden; }
```

---

## 3. THEME SYSTEM — ALL 7 THEMES

### Theme Engine: frontend/src/themes/mod.rs

```rust
use strum::{Display, EnumString};
use web_sys::CssStyleDeclaration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ThemeType { Dark, Light }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub bg: String, pub bg_secondary: String, pub bg_tertiary: String, pub bg_hover: String,
    pub border: String, pub border_active: String,
    pub text: String, pub text_muted: String, pub text_dim: String,
    pub accent: String, pub accent_hover: String, pub accent_subtle: String, pub accent_teal: String,
    pub success: String, pub error: String, pub warning: String,
    pub terminal_bg: String, pub terminal_fg: String, pub terminal_cursor: String,
    pub terminal_selection: String,
    pub glow_color: String,        // Atmosphere: lamp glow color (rgba)
    pub noise_opacity: f32,        // Atmosphere: grain opacity
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeDefinition {
    pub name: ThemeName, pub label: String,
    pub theme_type: ThemeType, pub colors: ThemeColors,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum ThemeName {
    Nyx, Aegis, Erebus, Pentelic, Olive, Sky, System,
}

impl Default for ThemeName {
    fn default() -> Self { ThemeName::Nyx }
}
```

### ALL THEME COLOR VALUES (exact hex/rgba):

#### ── Nyx (default) — obsidian + bronze-gold
| Token | Value |
|-------|-------|
| bg | `#0E0E11` |
| bg_secondary | `#16161A` |
| bg_tertiary | `#1E1E23` |
| bg_hover | `#26262C` |
| border | `#2A2A31` |
| border_active | `#3A3A43` |
| text | `#ECEAE3` |
| text_muted | `#9A968C` |
| text_dim | `#6A675E` |
| accent | `#C9A24B` |
| accent_hover | `#E0BC6A` |
| accent_subtle | `rgba(201, 162, 75, 0.12)` |
| accent_teal | `#4FA39E` |
| success | `#7BAE5A` |
| error | `#C5654D` |
| warning | `#D2973C` |
| terminal_bg | `#0E0E11` |
| terminal_fg | `#ECEAE3` |
| terminal_cursor | `#C9A24B` |
| terminal_selection | `rgba(201, 162, 75, 0.22)` |
| glow_color | `rgba(201, 162, 75, 0.10)` |
| noise_opacity | `0.022` |

#### ── Aegis — deep Aegean blue-black + bronze, faint teal glow
| Token | Value |
|-------|-------|
| bg | `#0A0F18` |
| bg_secondary | `#111826` |
| bg_tertiary | `#18202F` |
| bg_hover | `#1F2940` |
| border | `#243044` |
| border_active | `#33425C` |
| text | `#E6ECF2` |
| text_muted | `#8696AC` |
| text_dim | `#5A6678` |
| accent | `#CBA257` |
| accent_hover | `#E2BD78` |
| accent_subtle | `rgba(203, 162, 87, 0.13)` |
| accent_teal | `#56B0AA` |
| success | `#6FAE7A` |
| error | `#C5654D` |
| warning | `#D2973C` |
| terminal_bg | `#0A0F18` |
| terminal_fg | `#E6ECF2` |
| terminal_cursor | `#CBA257` |
| terminal_selection | `rgba(86, 176, 170, 0.22)` |
| glow_color | `rgba(86, 176, 170, 0.10)` |
| noise_opacity | `0.02` |

#### ── Erebus — true black + gold leaf (maximum contrast)
| Token | Value |
|-------|-------|
| bg | `#060607` |
| bg_secondary | `#0E0E10` |
| bg_tertiary | `#161618` |
| bg_hover | `#1E1E20` |
| border | `#232325` |
| border_active | `#343437` |
| text | `#EDEAE0` |
| text_muted | `#8E8A80` |
| text_dim | `#5E5A52` |
| accent | `#D8B765` |
| accent_hover | `#ECD089` |
| accent_subtle | `rgba(216, 183, 101, 0.12)` |
| success | `#7BAE5A` |
| glow_color | `rgba(216, 183, 101, 0.08)` |
| noise_opacity | `0.026` |

#### ── Pentelic — Pentelic marble + ink + terracotta-bronze
| Token | Value |
|-------|-------|
| bg | `#F6F4EE` |
| bg_secondary | `#EFECE3` |
| bg_tertiary | `#E6E2D6` |
| bg_hover | `#DCD7C8` |
| border | `#DAD5C7` |
| border_active | `#C3BCA8` |
| text | `#211E18` |
| text_muted | `#6A6456` |
| text_dim | `#9A9484` |
| accent | `#A8742F` |
| accent_hover | `#C08A40` |
| accent_subtle | `rgba(168, 116, 47, 0.14)` |
| accent_teal | `#2F7E79` |
| success | `#3E7A33` |
| error | `#B14530` |
| warning | `#B0791E` |
| terminal_bg | `#F6F4EE` |
| terminal_fg | `#211E18` |
| terminal_cursor | `#A8742F` |
| terminal_selection | `rgba(168, 116, 47, 0.16)` |
| glow_color | `rgba(168, 116, 47, 0.06)` |
| noise_opacity | `0.016` |

#### ── Olive — warm parchment + olive-gold + bronze
| Token | Value |
|-------|-------|
| bg | `#F3F1E7` |
| bg_secondary | `#EBE8DB` |
| bg_tertiary | `#E1DDCC` |
| bg_hover | `#D6D1BC` |
| border | `#D3CDB8` |
| border_active | `#BDB69C` |
| text | `#232117` |
| text_muted | `#6B6550` |
| text_dim | `#9C9578` |
| accent | `#8A7320` |
| accent_hover | `#A2882C` |
| accent_subtle | `rgba(138, 115, 32, 0.14)` |
| success | `#4A7A2C` |
| glow_color | `rgba(138, 115, 32, 0.06)` |
| noise_opacity | `0.016` |

#### ── Sky — cool marble + Aegean teal (cool light theme)
| Token | Value |
|-------|-------|
| bg | `#F7F9FB` |
| bg_secondary | `#EEF1F5` |
| bg_tertiary | `#E3E8EF` |
| bg_hover | `#D7DEE8` |
| border | `#D6DCE4` |
| border_active | `#BCC6D2` |
| text | `#16202E` |
| text_muted | `#566070` |
| text_dim | `#8A93A2` |
| accent | `#1F6F8B` |
| accent_hover | `#2A86A6` |
| accent_subtle | `rgba(31, 111, 139, 0.12)` |
| accent_teal | `#1F6F8B` |
| success | `#2F7D45` |
| error | `#B14536` |
| warning | `#B07A1E` |
| glow_color | `rgba(31, 111, 139, 0.07)` |
| noise_opacity | `0.014` |

### HOW THEME ENAPPLY works:
1. Backend stores theme name in KeyValueStore
2. On mount, `src/lib.rs` reads `theme` from store and calls `themes::apply_theme_to_dom(&theme_name)`
3. `apply_theme_to_dom` reads `ThemeColors` via `get_theme()`, then sets CSS custom properties on `document.documentElement` using `web_sys::CssStyleDeclaration::set_property`
4. Also sets `data-theme="<name>"` on `<html>`
5. Also calls `set_data_theme()` which sets `data-theme` attribute

---

## 4. LAYOUT ARCHITECTURE

### Overall App Layout (described in inline styles in lib.rs):
```
App (100vw × 100vh, flex-col):
  ├── Titlebar (38px height, flex-shrink: 0, -webkit-app-region: drag)
  │     ├── Mac traffic lights spacer (80px) [when is_mac]
  │     ├── Workspace Tabs (flex: 1, centered)
  │     └── Toolbar (flex-shrink: 0, icon buttons)
  │           ├── Panel switcher (workspace/kanban/swarm tabs)
  │           ├── Add Shell button
  │           ├── Athena toggle (Cmd+J)
  │           ├── Swarm launch
   │           ├── Notification bell
  │           └── Settings
  ├── Main Content (flex: 1, flex-row, overflow: hidden)
  │     ├── Sidebar or SidebarRail (collapsed)
  │     ├── Center Content (flex: 1, flex-col)
  │     │     ├── Active Panel (flex: 1, min-height: 0)
  │     │     │     ├── Panel::Workspace → WorkspaceGrid
  │     │     │     ├── Panel::Editor → "Editor panel"
  │     │     │     ├── Panel::Kanban → KanbanBoard
  │     │     │     ├── Panel::Swarm → SwarmBoard
  │     │     │     └── Panel::Settings → SettingsPanel
  │     │     └── RightSidebar (resizable, 480px default, 200-900px clamp)
  │     └── AgentInspector (absolute overlay)
  └── Status Bar (24px height, flex-shrink: 0)
        "workspace name · panes · panel"
```

### WorkspaceGrid Layout:
```
workspace-grid-root (flex: 1, flex-col, overflow: hidden, position: relative)
  └── for each row:
        div (flex-row, flex: <row_weight>, min-height: 0)
          └── for each pane in row:
                div (flex: <col_weight>, position: relative, min-width/height: 0)
                  ├── pane-focus-ring (when active, not fullscreen)
                  └── PaneItem (flex-col, fills space)
                        ├── Pill Header (flex-shrink: 0)
                        └── Shell Body (xterm or terminal grid)
```

### Pane Divider System:
- **ColDivider**: 8px invisible grab area with 1px visible line at center
  - position: relative; left: -4px; width: 8px; cursor: col-resize
- **RowDivider**: 8px invisible grab area with 1px visible line at center
  - position: relative; top: -4px; height: 8px; cursor: row-resize
- **Drag overlay**: fixed full-screen during resize, z-index: 9999

### Pane Focus Ring:
```css
.pane-focus-ring {
  position: absolute; inset: 0;
  pointer-events: none; z-index: 4;
  box-shadow: inset 0  With this established, I can now implement the UI enhancements. long example text that demonstrates an edge case in input handling where special characters cause display issues in terminal output.  \x1b  \x01  \x02  \x03  \x04  \x05  \x06  \x07
 [UI TWIN PACKAGE — END]
