//! Startup effects for the root application component.
//!
//! This hook groups persistence and window bootstrap work without owning any
//! rendered state. It must be called once, in the same position in `App`, so
//! Dioxus hook ordering remains stable.

use crate::stores::ui::UITheme;
use crate::stores::command::{use_command_store, CommandState};
use crate::stores::workspace::WorkspaceState;
use crate::utils::font_size::{parse_persisted_font_size, persist_font_size};
use crate::utils::settings_migration::migrate_smart_pane_titles;
use dioxus::prelude::*;

/// Run the root app's one-time platform, settings, workspace, and
/// before-unload bootstrap effects.
pub fn use_startup_bootstrap(
    ui_state: Signal<crate::stores::ui::UIState>,
    workspace: Signal<WorkspaceState>,
    mut platform: Signal<String>,
    mut is_maximized: Signal<bool>,
) {
    // Override platform with authoritative value from Tauri backend.
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

    // Sync maximized state on mount and after native resize events.
    use_effect(move || {
        spawn(async move {
            if let Ok(maximized) = crate::tauri_bridge::window_is_maximized().await {
                is_maximized.set(maximized);
            }
        });
    });

    use_effect(move || {
        spawn(async move {
            let last_call = std::rc::Rc::new(std::cell::Cell::new(0.0f64));
            // Raw Tauri callbacks run outside a Dioxus scope; use the bare
            // wasm executor for the follow-up IPC call.
            let _unlisten = crate::tauri_bridge::listen("tauri://resize", move |_payload| {
                let now = js_sys::Date::now();
                if now - last_call.get() < 150.0 {
                    return;
                }
                last_call.set(now);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(maximized) = crate::tauri_bridge::window_is_maximized().await {
                        is_maximized.set(maximized);
                    }
                });
            });
        });
    });

    // Load persisted settings and migrate old title keys.
    {
        let mut ui = ui_state;
        use_effect(move || {
            spawn(async move {
                if let Ok(theme_name) = crate::tauri_bridge::store_get("theme").await {
                    if !theme_name.is_empty() {
                        let theme = UITheme::from_name(&theme_name);
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
                    if let Some(size) = parse_persisted_font_size(&font_size_str) {
                        ui.write().font_size = size;
                        let family = ui.read().font_family.clone();
                        crate::themes::apply_font_to_dom(&family, size);
                        if font_size_str.trim() != size.to_string() {
                            persist_font_size(size);
                        }
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
                ui.write().smart_pane_titles = migrate_smart_pane_titles().await;
            });
        });
    }

    // Apply current local settings immediately while persistence loads.
    {
        let theme_name = ui_state.read().theme.name().to_string();
        let font_family = ui_state.read().font_family.clone();
        let font_size = ui_state.read().font_size;
        use_effect(move || {
            crate::themes::apply_theme_to_dom(&theme_name);
            crate::themes::apply_font_to_dom(&font_family, font_size);
        });
    }

    // Restore persisted workspaces and re-authorize their directories.
    {
        let mut ws = workspace;
        use_effect(move || {
            spawn(async move {
                let loaded = WorkspaceState::load().await;
                let resume_panes = loaded
                    .spaces
                    .iter()
                    .flat_map(|space| space.panes.iter())
                    .filter(|pane| pane.resume_id.is_some() || pane.resume_cmd.is_some())
                    .count();
                web_sys::console::log_1(
                    &format!(
                        "[resume-debug] startup loaded spaces={} panes={} resume_panes={} active={:?}",
                        loaded.spaces.len(),
                        loaded.spaces.iter().map(|space| space.panes.len()).sum::<usize>(),
                        resume_panes,
                        loaded.active_space_id
                    )
                    .into(),
                );
                for space in &loaded.spaces {
                    let dir = space.dir.trim();
                    if dir.is_empty() {
                        continue;
                    }
                    if let Err(e) = crate::tauri_bridge::workspace_add_trusted_root(dir).await {
                        web_sys::console::warn_1(
                            &format!("[startup] failed to re-trust space dir '{}': {:?}", dir, e)
                                .into(),
                        );
                    }
                }
                let mut state = ws.write();
                if state.spaces.is_empty() && state.active_space_id.is_none() {
                    web_sys::console::log_1(
                        &"[resume-debug] startup applying loaded workspace state".into(),
                    );
                    *state = loaded;
                } else {
                    web_sys::console::warn_1(
                        &format!(
                            "[resume-debug] startup skipped loaded workspace state; current spaces={} active={:?}",
                            state.spaces.len(),
                            state.active_space_id
                        )
                        .into(),
                    );
                }
            });
        });
    }

    // Restore recent command IDs.
    {
        let mut cmd = use_command_store();
        use_effect(move || {
            spawn(async move {
                cmd.write().recent_ids = CommandState::load_recent().await;
            });
        });
    }

    // Force a final workspace save before the window closes.
    {
        let final_ws = workspace;
        use_effect(move || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let ws_signal = final_ws;
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
}
