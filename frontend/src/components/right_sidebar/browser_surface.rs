//! Shared embedded-browser surface.
//!
//! Renders the browser toolbar (back / forward / reload / URL bar) plus a
//! placeholder `<div>` that a **native Tauri child webview** is overlaid on.
//! The iframe approach is impossible because `X-Frame-Options` blocks all major
//! sites (Google, GitHub, ...), so the real page is a separate WebKit process
//! living inside the same window.
//!
//! There is exactly **one** child webview (label derived from [`BROWSER_ID`]).
//! This component owns positioning: it measures the placeholder's on-screen rect
//! and tells the backend to size/position the webview to match, so it tracks the
//! resizable sidebar, the main-area panel, and window resizes.
//!
//! The same component is used in two places — the right sidebar
//! (`expanded == false`) and the main content area (`expanded == true`). The
//! `Panel::Browser == ui_state.panel` flag is the single source of truth for
//! which one is mounted, so only one surface exists at a time (see `panel.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::shared::icon::{
    IconArrowLeft, IconArrowRight, IconFullscreen, IconGlobe, IconMinimize, IconRefresh,
};
use crate::stores::panel_manager::{use_panel_manager_store, RightPanel};
use crate::stores::ui::{use_ui_store, Panel};
use crate::tauri_bridge;

/// Logical id of the single shared browser panel (backend derives the child
/// webview label `browser-child-{id}` from this).
pub const BROWSER_ID: &str = "sidebar-browser";
const DEFAULT_URL: &str = "https://www.google.com";
/// DOM id of the placeholder the native webview is overlaid on.
const VIEWPORT_ID: &str = "browser-surface-viewport";

thread_local! {
    /// Pending "park the webview off-screen" timeout. A short delay lets a
    /// surface that is *relocating* (sidebar → main area) cancel the park its
    /// predecessor scheduled, so the webview never blanks during a move.
    static PARK_TIMER: RefCell<Option<i32>> = const { RefCell::new(None) };
}

/// Holds JS resources that must be released when the surface unmounts.
struct SurfaceCleanup {
    resize_observer: Option<JsValue>,
    _ro_closure: Option<Closure<dyn FnMut()>>,
    window_resize_closure: Option<Closure<dyn FnMut()>>,
}

/// Read the placeholder's viewport rect and push it to the backend.
fn push_bounds_now() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(el) = document.get_element_by_id(VIEWPORT_ID) else {
        return;
    };
    let rect = el.get_bounding_client_rect();
    let (x, y, w, h) = (rect.left(), rect.top(), rect.width(), rect.height());
    // Skip degenerate rects (panel not laid out yet / hidden).
    if w <= 1.0 || h <= 1.0 {
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_bridge::browser_set_bounds(BROWSER_ID, x, y, w, h).await;
    });
}

/// Measure on the next animation frame (after layout has settled).
fn schedule_push_bounds() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let cb = Closure::once_into_js(move || push_bounds_now());
    let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
}

/// Move the webview far off-screen, keeping the page alive while hidden.
fn park_offscreen() {
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_bridge::browser_set_bounds(BROWSER_ID, -20000.0, -20000.0, 800.0, 600.0).await;
    });
}

/// Cancel any pending off-screen park (called when a surface mounts).
fn cancel_park() {
    PARK_TIMER.with(|t| {
        if let Some(id) = t.borrow_mut().take() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
        }
    });
}

/// Schedule an off-screen park shortly after a surface unmounts. A mounting
/// surface cancels it, so relocation never hides the webview.
fn request_park() {
    let Some(window) = web_sys::window() else {
        return;
    };
    cancel_park();
    let cb = Closure::once_into_js(move || {
        PARK_TIMER.with(|t| {
            *t.borrow_mut() = None;
        });
        park_offscreen();
    });
    if let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        cb.as_ref().unchecked_ref(),
        80,
    ) {
        PARK_TIMER.with(|t| {
            *t.borrow_mut() = Some(id);
        });
    }
}

/// The embedded browser surface. `expanded` selects sidebar (false) vs
/// main-area (true) presentation and flips the expand/dock toggle.
#[component]
pub fn BrowserSurface(expanded: bool) -> Element {
    let url = use_signal(|| DEFAULT_URL.to_string());
    let mut url_input = use_signal(|| DEFAULT_URL.to_string());
    let mut ui_state = use_ui_store();
    let mut panel_state = use_panel_manager_store();
    let mut cleanup: Signal<Option<SurfaceCleanup>> = use_signal(|| None);
    let initialized = use_hook(|| Rc::new(RefCell::new(false)));

    let quick_urls: Vec<(&str, &str)> = vec![
        ("Google", "https://www.google.com"),
        ("GitHub", "https://github.com"),
        ("Rust", "https://doc.rust-lang.org"),
        ("React", "https://react.dev"),
        ("MDN", "https://developer.mozilla.org"),
    ];
    let localhost_urls: Vec<(&str, &str)> = vec![
        (":3000", "http://localhost:3000"),
        (":5173", "http://localhost:5173"),
        (":8080", "http://localhost:8080"),
        (":8000", "http://localhost:8000"),
        (":4200", "http://localhost:4200"),
        (":5000", "http://localhost:5000"),
    ];

    // ── Mount once: create the webview, observe size, listen for resize ──────
    {
        let initialized = initialized.clone();
        let start_url = url();
        use_effect(move || {
            if *initialized.borrow() {
                return;
            }
            *initialized.borrow_mut() = true;

            // A surface is appearing — cancel any park a predecessor scheduled.
            cancel_park();

            let Some(window) = web_sys::window() else {
                return;
            };

            // Create (idempotent) the child webview, then position it.
            let create_url = start_url.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = tauri_bridge::browser_show(BROWSER_ID, &create_url).await;
                schedule_push_bounds();
            });

            // ResizeObserver on the placeholder catches sidebar drag-resize and
            // panel size changes.
            let ro_closure = Closure::wrap(Box::new(move || {
                schedule_push_bounds();
            }) as Box<dyn FnMut()>);
            let resize_observer =
                js_sys::Reflect::get(&window, &JsValue::from_str("ResizeObserver"))
                    .ok()
                    .and_then(|c| c.dyn_into::<js_sys::Function>().ok())
                    .and_then(|c| {
                        js_sys::Reflect::construct(&c, &js_sys::Array::of1(ro_closure.as_ref())).ok()
                    });
            if let (Some(observer), Some(document)) =
                (resize_observer.as_ref(), window.document())
            {
                if let Some(el) = document.get_element_by_id(VIEWPORT_ID) {
                    if let Ok(observe_fn) =
                        js_sys::Reflect::get(observer, &JsValue::from_str("observe"))
                    {
                        if let Ok(observe_fn) = observe_fn.dyn_into::<js_sys::Function>() {
                            let _ = observe_fn.call1(observer, el.as_ref());
                        }
                    }
                }
            }

            // Window resize catches OS-level window resize / maximize.
            let win_resize = Closure::wrap(Box::new(move || {
                schedule_push_bounds();
            }) as Box<dyn FnMut()>);
            let _ = window
                .add_event_listener_with_callback("resize", win_resize.as_ref().unchecked_ref());

            cleanup.set(Some(SurfaceCleanup {
                resize_observer,
                _ro_closure: Some(ro_closure),
                window_resize_closure: Some(win_resize),
            }));
        });
    }

    // ── Reconcile bounds whenever layout-affecting state changes ─────────────
    // Position (not just size) shifts when the left sidebar opens/closes or the
    // active panel changes; ResizeObserver alone misses pure moves.
    use_effect(move || {
        let _ = ui_state.read().sidebar_visible;
        let _ = ui_state.read().sidebar_width;
        let _ = ui_state.read().right_sidebar_open;
        let _ = ui_state.read().panel;
        let _ = panel_state.read().active_right_panel;
        let _ = panel_state.read().right_panel_width_percent;
        schedule_push_bounds();
    });

    // ── Unmount: release JS resources, schedule off-screen park ──────────────
    use_drop(move || {
        if let Some(mut c) = cleanup.take() {
            if let Some(observer) = c.resize_observer.take() {
                if let Ok(disc) =
                    js_sys::Reflect::get(&observer, &JsValue::from_str("disconnect"))
                {
                    if let Ok(disc) = disc.dyn_into::<js_sys::Function>() {
                        let _ = disc.call0(&observer);
                    }
                }
            }
            if let (Some(window), Some(cl)) =
                (web_sys::window(), c.window_resize_closure.take())
            {
                let _ = window.remove_event_listener_with_callback(
                    "resize",
                    cl.as_ref().unchecked_ref(),
                );
            }
        }
        request_park();
    });

    let toggle_title = if expanded {
        "Dock to sidebar"
    } else {
        "Expand to main area"
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; min-height: 0;",

            // ── Toolbar ──────────────────────────────────────────────────────
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                button {
                    class: "icon-btn",
                    title: "Back",
                    onclick: move |_| {
                        let mut url_clone = url;
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_back(BROWSER_ID).await {
                                Ok(new_url) => url_clone.set(new_url),
                                Err(e) => web_sys::console::warn_1(&JsValue::from_str(&format!("Back: {:?}", e))),
                            }
                        });
                    },
                    IconArrowLeft { size: Some(16), color: Some("currentColor".to_string()) }
                }

                button {
                    class: "icon-btn",
                    title: "Forward",
                    onclick: move |_| {
                        let mut url_clone = url;
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_forward(BROWSER_ID).await {
                                Ok(new_url) => url_clone.set(new_url),
                                Err(e) => web_sys::console::warn_1(&JsValue::from_str(&format!("Forward: {:?}", e))),
                            }
                        });
                    },
                    IconArrowRight { size: Some(16), color: Some("currentColor".to_string()) }
                }

                button {
                    class: "icon-btn",
                    title: "Reload",
                    onclick: move |_| {
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ = tauri_bridge::browser_reload(BROWSER_ID).await;
                        });
                    },
                    IconRefresh { size: Some(16), color: Some("currentColor".to_string()) }
                }

                input {
                    class: "field",
                    style: "flex: 1; min-width: 0;",
                    value: "{url_input}",
                    oninput: move |e| url_input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            let trimmed = url_input().trim().to_string();
                            if !trimmed.is_empty() {
                                let mut url_clone = url;
                                let mut input_clone = url_input;
                                wasm_bindgen_futures::spawn_local(async move {
                                    match tauri_bridge::browser_navigate(BROWSER_ID, &trimmed).await {
                                        Ok(_) => {
                                            url_clone.set(trimmed.clone());
                                            input_clone.set(trimmed);
                                        }
                                        Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e))),
                                    }
                                });
                            }
                        }
                    },
                    placeholder: "Enter URL or search..."
                }

                // Expand / dock toggle
                button {
                    class: "icon-btn",
                    title: "{toggle_title}",
                    onclick: move |_| {
                        if expanded {
                            // Dock back into the right sidebar Browser tab.
                            ui_state.write().panel = Panel::Workspace;
                            ui_state.write().right_sidebar_open = true;
                            panel_state.write().open_right_panel(RightPanel::Browser);
                        } else {
                            // Pop out to the main content area.
                            ui_state.write().panel = Panel::Browser;
                        }
                    },
                    if expanded {
                        IconMinimize { size: Some(16), color: Some("currentColor".to_string()) }
                    } else {
                        IconFullscreen { size: Some(16), color: Some("currentColor".to_string()) }
                    }
                }
            }

            // ── Native webview viewport (overlaid by the child webview) ──────
            div {
                id: "{VIEWPORT_ID}",
                style: "flex: 1; min-height: 0; background: var(--bgTertiary); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--textDim); padding: 24px; text-align: center;",
                span {
                    style: "opacity: 0.35;",
                    IconGlobe { size: Some(40), color: Some("var(--textDim)".to_string()) }
                }
                div {
                    style: "font-family: var(--font-display); font-size: 16px; font-weight: 600; color: var(--text);",
                    "Loading browser…"
                }
                div {
                    style: "font-size: 11px; max-width: 280px; color: var(--textMuted);",
                    "Web content renders in a native view over this area. Use the URL bar or the shortcuts below."
                }
            }

            // ── Quick access ────────────────────────────────────────────────
            div {
                style: "border-top: 1px solid var(--border); padding: 8px 12px; background: var(--bgSecondary); flex-shrink: 0; display: flex; flex-direction: column; gap: 8px;",

                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    div {
                        style: "font-family: var(--font-display); font-size: 12px; font-weight: 600; letter-spacing: 0.02em; color: var(--textDim); white-space: nowrap; min-width: 70px;",
                        "Quick Access"
                    }
                    div {
                        style: "display: flex; flex-wrap: wrap; gap: 4px; flex: 1;",
                        for (name, url_str) in quick_urls.iter().cloned() {
                            button {
                                class: "card is-interactive",
                                style: "padding: 3px 8px; font-size: 10px; cursor: pointer; white-space: nowrap;",
                                onclick: move |_| {
                                    let target = url_str.to_string();
                                    let mut url_clone = url;
                                    let mut input_clone = url_input;
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match tauri_bridge::browser_navigate(BROWSER_ID, &target).await {
                                            Ok(_) => {
                                                url_clone.set(target.clone());
                                                input_clone.set(target);
                                            }
                                            Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e))),
                                        }
                                    });
                                },
                                "{name}"
                            }
                        }
                    }
                }

                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    div {
                        style: "font-family: var(--font-display); font-size: 12px; font-weight: 600; letter-spacing: 0.02em; color: var(--textDim); white-space: nowrap; min-width: 70px;",
                        "Localhost"
                    }
                    div {
                        style: "display: flex; flex-wrap: wrap; gap: 4px; flex: 1;",
                        for (label, url_str) in localhost_urls.iter().cloned() {
                            button {
                                class: "card is-interactive",
                                style: "padding: 3px 8px; font-size: 10px; cursor: pointer; white-space: nowrap;",
                                onclick: move |_| {
                                    let target = url_str.to_string();
                                    let mut url_clone = url;
                                    let mut input_clone = url_input;
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match tauri_bridge::browser_navigate(BROWSER_ID, &target).await {
                                            Ok(_) => {
                                                url_clone.set(target.clone());
                                                input_clone.set(target);
                                            }
                                            Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e))),
                                        }
                                    });
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
