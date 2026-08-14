#![allow(clippy::type_complexity)]
pub mod components;
pub mod stores;
pub mod tauri_bridge;
pub mod themes;
pub mod types;
pub mod utils;

use components::agents::agent_inspector::AgentInspector;
use components::agents::output_event_bus::OutputEventBus;
use components::command_palette::CommandPalette;
use components::kanban::kanban_board::KanbanBoard;
use components::mobile::{should_render_mobile_app, MobileApp};
use components::notifications::notification_bell::{
    provide_notification_overlay_store, NotificationBell, NotificationPopover,
};
use components::notifications::notification_panel::NotificationPanel;
use components::notifications::notification_toast::NotificationToast;
use components::plugin::plugin_event_bus::{provide_plugin_bus_store, PluginEventBus};
use components::right_sidebar::editor_panel::RightEditorPanel;
use components::right_sidebar::panel::RightSidebar;
use components::right_sidebar::BrowserSurface;
use components::settings::settings_modal::SettingsModal;
use components::settings::SettingsPanel;
use components::shared::error_boundary::ErrorBoundary;
use components::shared::icon::{
    IconAgents, IconAthena, IconChevronRight, IconClose, IconFiles, IconGrid, IconLaurel,
    IconMinus, IconPlugins, IconPlus, IconSettings, IconSwarm, IconWindowMaximize,
    IconWindowRestore,
};
use components::shared::illustration::CoreMark;
use components::shared::metrics_badge::MetricsBadge;
use components::shared::toast::{provide_toast_store, ToastContainer};
use components::sidebar::Sidebar;
use components::swarm::swarm_board::SwarmBoard;
use components::swarm::swarm_modal::SwarmModal;
use components::workspace::new_space_modal::NewSpaceModal;
use components::workspace::terminal_controller::TerminalController;
use components::workspace::terminal_grid::WorkspaceGrid;
use components::workspace::workspace_tabs::WorkspaceTabs;
use dioxus::prelude::*;
use gloo::timers::callback::Interval;
use std::cell::RefCell;
use std::rc::Rc;
use stores::agent_output::provide_agent_output_store;
use stores::agent_status::provide_agent_status_store;
use stores::athena::{provide_athena_store, use_athena_store};
use stores::command::provide_command_store;
use stores::editor::provide_editor_store;
use stores::notification::provide_notification_store;
use stores::panel_manager::{provide_panel_manager_store, use_panel_manager_store};
use stores::session::provide_session_store;
use stores::swarm::provide_swarm_store;
use stores::task::provide_task_store;
use stores::terminal::provide_terminal_store;
use stores::ui::{provide_ui_store, use_ui_store, Panel, SidebarSection};
use utils::font_size::{adjust_font_size, persist_font_size};
use utils::keybindings::{classify, GlobalKeyAction};
use utils::startup_bootstrap::use_startup_bootstrap;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use stores::workspace::{provide_workspace_store, use_workspace_store, Space};
use utils::pane_config::new_shell_pane;

/// Apply a shared font-size change immediately and persist it best-effort.
/// The reactive xterm effect observes `UIState.font_size` and updates every
/// mounted desktop terminal, including FitAddon recalculation and PTY resize.
fn change_shared_font_size(mut ui_state: Signal<crate::stores::ui::UIState>, delta: i8) {
    let current = ui_state.read().font_size;
    let next = adjust_font_size(current, delta);
    if next == current {
        return;
    }

    let family = ui_state.read().font_family.clone();
    ui_state.write().font_size = next;
    crate::themes::apply_font_to_dom(&family, next);

    persist_font_size(next);
}

/// Root application component — faithful port of App.tsx.
#[component]
pub fn App() -> Element {
    // Keep the outer JavaScript watchdog informed even while the DOM is idle.
    // This is intentionally outside the error-boundary subtree: if a Dioxus
    // render path fails, the heartbeat stops and the WebView-level recovery
    // path can take over. Interval is stored in the component hook so it is
    // cancelled automatically when the root is unmounted.
    // Performance instrumentation: count this component's renders and expose
    // the counters to WebDriver/console via `window.__athenaMetrics.snapshot()`.
    crate::utils::perf_metrics::mark_render("App");
    crate::utils::perf_metrics::install_window_snapshot();
    // Keep the snapshot fresh without per-render work (2s cadence).
    let _metrics_refresh = use_signal(|| {
        Interval::new(2_000, || {
            crate::utils::perf_metrics::refresh_window_snapshot();
        })
    });

    let _watchdog_heartbeat = use_signal(|| {
        Interval::new(5000, || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let _ = js_sys::Reflect::get(
                &window,
                &wasm_bindgen::JsValue::from_str("__athenaWasmHeartbeat"),
            )
            .ok()
            .and_then(|callback| callback.dyn_into::<js_sys::Function>().ok())
            .and_then(|callback| callback.call0(&window).ok());
        })
    });

    if should_render_mobile_app() {
        return rsx! {
            ErrorBoundary {
                fallback_message: "The mobile interface could not be rendered.".to_string(),
                MobileApp {}
            }
        };
    }

    // Provide all 14 stores
    provide_ui_store();
    provide_workspace_store();
    provide_athena_store();
    provide_notification_store();
    provide_notification_overlay_store();
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

    // Capture keyboard shortcuts before xterm.js receives them. The bubbling
    // Dioxus handler below still performs the app action; this listener only
    // prevents WebView defaults such as Cmd+W closing the app and keeps
    // command-key presses from entering an interactive agent TUI.
    let shortcut_capture: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::KeyboardEvent)>>>> =
        use_hook(|| Rc::new(RefCell::new(None)));
    {
        let shortcut_capture = shortcut_capture.clone();
        use_effect(move || {
            if shortcut_capture.borrow().is_some() {
                return;
            }
            let Some(window) = web_sys::window() else {
                return;
            };
            let handler = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                if (!event.meta_key() && !event.ctrl_key()) || event.alt_key() {
                    return;
                }
                let key = event.key();
                if matches!(
                    key.as_str(),
                    "k" | "p"
                        | "j"
                        | "\\"
                        | "t"
                        | "b"
                        | "1"
                        | "2"
                        | "3"
                        | "4"
                        | "w"
                        | "e"
                        | ","
                        | "A"
                        | "S"
                        | "P"
                        | "R"
                        | "="
                        | "+"
                        | "-"
                ) {
                    event.prevent_default();
                }
            }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
            let _ = window.add_event_listener_with_callback_and_bool(
                "keydown",
                handler.as_ref().unchecked_ref(),
                true,
            );
            *shortcut_capture.borrow_mut() = Some(handler);
        });
    }
    {
        let shortcut_capture = shortcut_capture.clone();
        use_drop(move || {
            if let (Some(window), Some(handler)) =
                (web_sys::window(), shortcut_capture.borrow_mut().take())
            {
                let _ = window.remove_event_listener_with_callback_and_bool(
                    "keydown",
                    handler.as_ref().unchecked_ref(),
                    true,
                );
            }
        });
    }

    // Track mounted spaces. The set is reconciled against the current
    // workspace state on every render so it stays bounded — entries for
    // spaces that no longer exist are dropped, preventing unbounded growth
    // across long sessions that create and destroy many workspaces.
    let mounted_spaces = use_signal(std::collections::HashSet::<String>::new);
    let platform = use_signal(|| {
        if crate::utils::platform_utils::is_mac() {
            "MacIntel"
        } else {
            "unknown"
        }
        .to_string()
    });
    let mut is_maximized = use_signal(|| false);
    // Lightweight cross-component command channel. TerminalController owns
    // terminal-store reads; App only increments this counter for Cmd+W.
    let mut close_first_pane_request = use_signal(|| 0_u64);

    // ─── Resizable right sidebar state ─────────────────────────────────
    let mut right_sidebar_width = use_signal(|| 480i32);
    let mut rsb_drag_start_x = use_signal(|| 0i32);
    let mut rsb_drag_start_w = use_signal(|| 0i32);
    let mut rsb_is_dragging = use_signal(|| false);

    use_startup_bootstrap(ui_state, workspace, platform, is_maximized);

    // Read-only access for rendering — all mounted_spaces mutations happen
    // inside use_effect (after render) to avoid the write-during-render
    // anti-pattern that triggers infinite re-render loops in Dioxus.
    let active_space_id = workspace.read().active_space_id.clone();
    let spaces = workspace.read().spaces.clone();

    // Sync mounted_spaces inside use_effect so writes happen after render.
    use_effect({
        let mut mounted_spaces = mounted_spaces;
        move || {
            let ws = workspace.read();
            let existing_ids: std::collections::HashSet<String> =
                ws.spaces.iter().map(|s| s.id.clone()).collect();
            let active = ws.active_space_id.clone();

            let mut mounted = mounted_spaces.write();

            // Track active space
            if let Some(id) = &active {
                mounted.insert(id.clone());
            }

            // Prune spaces that no longer exist
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
                let key = e.key();
                let action = classify(&key, e.modifiers());
                // Never let the WebView/browser consume an application shortcut
                // (notably Cmd+W, which otherwise closes the window after the
                // renderer has already started a pane-remount). Preventing the
                // default before any signal mutation also avoids the visible
                // lag/close sequence reported on macOS.
                if action.is_some() {
                    e.prevent_default();
                }
                match action {
                    Some(GlobalKeyAction::IncreaseFontSize) => {
                        change_shared_font_size(ui_state, 1);
                    }
                    Some(GlobalKeyAction::DecreaseFontSize) => {
                        change_shared_font_size(ui_state, -1);
                    }
                    Some(GlobalKeyAction::ToggleCommandPalette) => {
                        let v = ui_state.read().command_palette_open;
                        ui_state.write().command_palette_open = !v;
                    }
                    Some(GlobalKeyAction::ToggleRightSidebar) => {
                        let is_open = ui_state.read().right_sidebar_open;
                        let should_be_open = panel_state.write().toggle_right_sidebar(is_open);
                        ui_state.write().right_sidebar_open = should_be_open;
                    }
                    Some(GlobalKeyAction::ShowNewSpace) => {
                        ui_state.write().show_new_space_modal = true;
                    }
                    Some(GlobalKeyAction::ToggleSidebar) => {
                        let v = ui_state.read().sidebar_visible;
                        ui_state.write().sidebar_visible = !v;
                    }
                    Some(GlobalKeyAction::SetWorkspacePanel) => {
                        ui_state.write().panel = Panel::Workspace;
                    }
                    Some(GlobalKeyAction::SetEditorPanel) => {
                        ui_state.write().panel = Panel::Editor;
                    }
                    Some(GlobalKeyAction::SetKanbanPanel) => {
                        ui_state.write().panel = Panel::Kanban;
                    }
                    Some(GlobalKeyAction::SetSwarmPanel) => {
                        ui_state.write().panel = Panel::Swarm;
                    }
                    Some(GlobalKeyAction::CloseFirstPane) => {
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Some(active) = doc.active_element() {
                                    let tag = active.tag_name().to_lowercase();
                                    let is_editable = tag == "input"
                                        || tag == "textarea"
                                        || active.get_attribute("contenteditable").is_some();
                                    if is_editable {
                                        return;
                                    }
                                }
                            }
                        }
                        let next_request = close_first_pane_request().wrapping_add(1);
                        close_first_pane_request.set(next_request);
                    }
                    Some(GlobalKeyAction::ToggleEditorPanel) => {
                        let current = ui_state.read().panel;
                        ui_state.write().panel =
                            if current == Panel::Editor { Panel::Workspace } else { Panel::Editor };
                    }
                    Some(GlobalKeyAction::ShowSettings) => {
                        ui_state.write().show_settings_modal = true;
                    }
                    Some(GlobalKeyAction::ShowSwarmModal) => {
                        ui_state.write().show_swarm_modal = true;
                    }
                    Some(GlobalKeyAction::AddShell) => {
                        let active_id = workspace.read().active_space_id.clone();
                        if let Some(sid) = active_id {
                            let pane = new_shell_pane();
                            workspace_mut.write().add_pane_to_space(&sid, pane);
                            e.prevent_default();
                        }
                    }
                    Some(GlobalKeyAction::ResetWorkspaceView) => {
                        ui_state.write().sidebar_width = 240.0;
                        ui_state.write().panel = Panel::Workspace;
                        ui_state.write().sidebar_section = SidebarSection::Spaces;
                    }
                    Some(GlobalKeyAction::Escape) => {
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Some(active) = doc.active_element() {
                                    let tag = active.tag_name().to_lowercase();
                                    let is_editable = tag == "input"
                                        || tag == "textarea"
                                        || active.get_attribute("contenteditable").is_some();
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
                        athena_state.write().is_open = false;
                        e.stop_propagation();
                    }
                    None => {}
                }
            },

            // Title bar
            div {
                class: "titlebar reveal-1",
                style: "height: var(--tb-height); -webkit-app-region: drag; display: flex; align-items: center; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                // Mac spacer for traffic lights
                if is_mac {
                    div { style: "width: 80px; flex-shrink: 0;" }
                }

                // Brand seal + wordmark — inside the drag region, never blocks clicks.
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 0 14px; -webkit-app-region: drag; pointer-events: none;",
                    span {
                        class: "seal-mark",
                        IconAthena { size: Some(18), color: Some("var(--accent)".to_string()) }
                    }
                    span {
                        style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; letter-spacing: 0.14em; color: var(--accent); text-transform: uppercase;",
                        "Athena"
                    }
                }

                // Workspace tabs (centered, flex-1)
                div { style: "flex: 1; display: flex; align-items: center; justify-content: center; gap: 4px; padding: 0 8px; min-width: 0; overflow: hidden;",
                    WorkspaceTabs { on_new_space: move |_| { ui_state.write().show_new_space_modal = true; } }
                }

                // Right toolbar buttons
                div { style: "display: flex; align-items: center; gap: 4px; padding-right: 14px; flex-shrink: 0; -webkit-app-region: no-drag;",

                    // Panel switcher (only when a workspace is active)
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
                                            style: "height: var(--tb-tab-height); padding: 0 12px; border: none; background: transparent; color: {color}; cursor: pointer; font-size: var(--tb-tab-font); font-weight: {weight}; letter-spacing: 0.04em; text-transform: uppercase;",
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
                            class: "icon-btn tb-extra-btn",
                            title: "Add Shell (Cmd+Shift+A)",
                            onclick: move |_| {
                                let active_id = {
                                    let ws = workspace.read();
                                    ws.active_space_id.clone()
                                };
                                if let Some(sid) = active_id {
                                    let pane = new_shell_pane();
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
                            let is_open = ui_state.read().right_sidebar_open;
                            let should_be_open = panel_state.write().toggle_right_sidebar(is_open);
                            ui_state.write().right_sidebar_open = should_be_open;
                        },
                        IconAthena { size: Some(16), color: Some("currentColor".to_string()) }
                    }

                    // Swarm launch
                    button {
                        class: "icon-btn tb-extra-btn",
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
                            IconMinus { size: Some(15), color: Some("currentColor".to_string()) }
                        }
                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--textMuted); cursor: pointer;",
                            onclick: move |_| {
                                let maximized = is_maximized();
                                is_maximized.set(!maximized);
                                spawn(async move { let _ = crate::tauri_bridge::window_maximize().await; });
                            },
                            if is_maximized() {
                                IconWindowRestore { size: Some(13), color: Some("currentColor".to_string()) }
                            } else {
                                IconWindowMaximize { size: Some(13), color: Some("currentColor".to_string()) }
                            }
                        }
                        button {
                            style: "height: 38px; width: 46px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--textMuted); cursor: pointer;",
                            onmouseover: move |e| { let _ = e; },
                            onclick: move |_| { spawn(async move { let _ = crate::tauri_bridge::window_close().await; }); },
                            IconClose { size: Some(14), color: Some("currentColor".to_string()) }
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
                            IconChevronRight { size: Some(15), color: Some("var(--textMuted)".to_string()) }
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
                                // Branded welcome — frost-heavy plaque over the starfield sky.
                                div {
                                    class: "animate-rise",
                                    style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 28px; padding: 40px; margin: 24px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bgSecondary); box-shadow: var(--shadow-lg);",

                                    div {
                                        style: "display: flex; flex-direction: column; align-items: center; gap: 14px;",

                                        // CoreMark carries the shared hexagonal core mark,
                                        // so no separate pendant is needed.
                                        div { CoreMark { size: Some(52) } }

                                        div { style: "display: flex; flex-direction: column; align-items: center; gap: 6px;",
                                            h2 {
                                                style: "font-family: var(--font-display); font-size: 34px; font-weight: 600; margin: 0; color: var(--accent); letter-spacing: 0.04em;",
                                                "Athena\u{2019}s Core"
                                            }
                                            p {
                                                style: "font-size: 14px; margin: 0; color: var(--textMuted);",
                                                "Open a workspace to start building."
                                            }
                                        }
                                    }

                                    // Laurel motif — transparent inline art, not a raster banner.
                                    div {
                                        class: "welcome-laurel",
                                        aria_hidden: "true",
                                        IconLaurel { size: Some(230), color: Some("var(--accent)".to_string()) }
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
                                    // Workspace branch is ALWAYS mounted and toggled
                                    // with display:none — switching to Kanban/Swarm/etc.
                                    // unmounts the WorkspaceGrid subtree otherwise, and
                                    // remounting hits per-pane Signal<TerminalSession>
                                    // reads whose generational-box backing storage was
                                    // reclaimed during the unmount, panicking with
                                    // `Err(Dropped(ValueDroppedError))` (wasm unreachable).
                                    // Other panels remain conditionally mounted (their
                                    // hooks are cheap and carry no cross-panel signal
                                    // refs that would suffer the same remount hazard).
                                    ErrorBoundary {
                                        fallback_message: "This workspace panel could not be rendered.".to_string(),
                                        div {
                                        style: if active_panel == Panel::Workspace {
                                            "flex: 1; display: flex; min-width: 0; min-height: 0; position: relative;"
                                        } else {
                                            "flex: 1; display: none; min-width: 0; min-height: 0; position: relative;"
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
                                    }
                                    if active_panel == Panel::Editor {
                                        ErrorBoundary {
                                            fallback_message: "The editor could not be rendered.".to_string(),
                                            div { class: "panel-enter", RightEditorPanel {} }
                                        }
                                    } else if active_panel == Panel::Kanban {
                                        ErrorBoundary {
                                            fallback_message: "The Kanban board could not be rendered.".to_string(),
                                            div { class: "panel-enter", KanbanBoard {} }
                                        }
                                    } else if active_panel == Panel::Swarm {
                                        ErrorBoundary {
                                            fallback_message: "The swarm board could not be rendered.".to_string(),
                                            div { class: "panel-enter", SwarmBoard {} }
                                        }
                                    } else if active_panel == Panel::Settings {
                                        ErrorBoundary {
                                            fallback_message: "The settings panel could not be rendered.".to_string(),
                                            div { class: "panel-enter", SettingsPanel {} }
                                        }
                                    } else if active_panel == Panel::Browser {
                                        ErrorBoundary {
                                            fallback_message: "The browser panel could not be rendered.".to_string(),
                                            // Embedded browser popped out to the main
                                            // content area (expanded mode). The native
                                            // child webview is overlaid on this surface.
                                            div { class: "panel-enter", BrowserSurface { expanded: true } }
                                        }
                                    } else if active_panel == Panel::Notifications {
                                        ErrorBoundary {
                                            fallback_message: "Notifications could not be rendered.".to_string(),
                                            div { class: "panel-enter", NotificationPanel {} }
                                        }
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
                MetricsBadge {}
                span {
                    style: "display: inline-flex; align-items: center; gap: 5px; color: var(--accent); font-weight: 600;",
                    span { style: "width: 5px; height: 5px; border-radius: 50%; background: var(--accent);" }
                    "{theme_label}"
                }
            }

            ErrorBoundary {
                fallback_message: "The command palette could not be rendered.".to_string(),
                CommandPalette {}
            }

            if ui_state.read().show_new_space_modal {
                ErrorBoundary {
                    fallback_message: "The workspace dialog could not be rendered.".to_string(),
                    NewSpaceModal {
                        on_close: move |_| { ui_state.write().show_new_space_modal = false; },
                    }
                }
            }

            if ui_state.read().show_swarm_modal {
                ErrorBoundary {
                    fallback_message: "The swarm dialog could not be rendered.".to_string(),
                    SwarmModal {
                        on_close: move |_| { ui_state.write().show_swarm_modal = false; },
                    }
                }
            }

            if ui_state.read().show_settings_modal {
                ErrorBoundary {
                    fallback_message: "The settings dialog could not be rendered.".to_string(),
                    SettingsModal {
                        on_close: move |_| { ui_state.write().show_settings_modal = false; },
                    }
                }
            }

            // Keep terminal coordination outside the root's reactive shell.
            TerminalController {
                close_request: close_first_pane_request,
            }

            // Notifications remain passive: attention is represented by the
            // agent status/badge and notification bell rather than a blocking
            // full-screen input-request modal.
            ErrorBoundary {
                fallback_message: "A background interface component could not be rendered.".to_string(),
                ToastContainer {}
                NotificationToast {}
                NotificationPopover {}
                PluginEventBus {}
                OutputEventBus {}
            }

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
