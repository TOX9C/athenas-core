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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::shared::icon::{
    IconArrowLeft, IconArrowRight, IconChevronDown, IconFullscreen, IconGlobe, IconMinimize,
    IconRefresh,
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
const LOAD_TIMEOUT_MS: i32 = 15_000;

#[derive(Clone, Copy)]
struct BrowserBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

struct BoundsDispatch {
    latest: Option<BrowserBounds>,
    request_in_flight: bool,
}

struct SurfaceLease {
    next_generation: u64,
    active_generation: Option<u64>,
    park_timer: Option<i32>,
}

thread_local! {
    /// Latest-only bounds state prevents resize bursts from applying stale
    /// rectangles after a newer IPC request has already been measured.
    static BOUNDS_DISPATCH: RefCell<BoundsDispatch> = const { RefCell::new(BoundsDispatch {
        latest: None,
        request_in_flight: false,
    }) };
    /// A generation lease prevents an old surface's drop hook from parking a
    /// newly mounted surface during sidebar/main-area relocation.
    static SURFACE_LEASE: RefCell<SurfaceLease> = const { RefCell::new(SurfaceLease {
        next_generation: 0,
        active_generation: None,
        park_timer: None,
    }) };
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
    let bounds = BrowserBounds {
        x: rect.left(),
        y: rect.top(),
        width: rect.width(),
        height: rect.height(),
    };
    // Skip degenerate rects (panel not laid out yet / hidden).
    if bounds.width <= 1.0 || bounds.height <= 1.0 {
        return;
    }
    enqueue_bounds(bounds);
}

fn send_bounds(bounds: BrowserBounds) {
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_bridge::browser_set_bounds(
            BROWSER_ID,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        )
        .await;

        let next = BOUNDS_DISPATCH.with(|dispatch| {
            let mut dispatch = dispatch.borrow_mut();
            dispatch.request_in_flight = false;
            dispatch.latest.take()
        });
        if let Some(next) = next {
            BOUNDS_DISPATCH.with(|dispatch| {
                dispatch.borrow_mut().request_in_flight = true;
            });
            send_bounds(next);
        }
    });
}

fn enqueue_bounds(bounds: BrowserBounds) {
    let next = BOUNDS_DISPATCH.with(|dispatch| {
        let mut dispatch = dispatch.borrow_mut();
        dispatch.latest = Some(bounds);
        if dispatch.request_in_flight {
            None
        } else {
            dispatch.request_in_flight = true;
            dispatch.latest.take()
        }
    });
    if let Some(next) = next {
        send_bounds(next);
    }
}

fn clear_pending_bounds() {
    BOUNDS_DISPATCH.with(|dispatch| {
        dispatch.borrow_mut().latest = None;
    });
}

fn clear_load_timeout(timeout: &Rc<RefCell<Option<i32>>>) {
    if let Some(timer) = timeout.borrow_mut().take() {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(timer);
        }
    }
}

fn schedule_load_timeout(
    timeout: Rc<RefCell<Option<i32>>>,
    active_generation: Rc<Cell<u64>>,
    generation: u64,
    mut loading: Signal<bool>,
    mut browser_error: Signal<Option<String>>,
) {
    clear_load_timeout(&timeout);
    let timeout_for_callback = timeout.clone();
    let callback = Closure::once_into_js(move || {
        *timeout_for_callback.borrow_mut() = None;
        if active_generation.get() != generation {
            return;
        }
        loading.set(false);
        browser_error.set(Some(
            "The page did not finish loading. Check the URL or retry.".to_string(),
        ));
    });
    if let Some(window) = web_sys::window() {
        if let Ok(timer) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            LOAD_TIMEOUT_MS,
        ) {
            *timeout.borrow_mut() = Some(timer);
        }
    }
}

/// Measure on the next animation frame (after layout has settled).
fn schedule_push_bounds() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let cb = Closure::once_into_js(push_bounds_now);
    let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
}

/// Move the webview far off-screen, keeping the page alive while hidden.
fn park_offscreen() {
    wasm_bindgen_futures::spawn_local(async move {
        let _ =
            tauri_bridge::browser_set_bounds(BROWSER_ID, -20000.0, -20000.0, 800.0, 600.0).await;
    });
}

/// Start a new surface lease and cancel a park scheduled by a predecessor.
fn begin_surface() -> u64 {
    let generation = SURFACE_LEASE.with(|lease| {
        let mut lease = lease.borrow_mut();
        if let Some(timer) = lease.park_timer.take() {
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(timer);
            }
        }
        lease.next_generation = lease.next_generation.wrapping_add(1);
        lease.active_generation = Some(lease.next_generation);
        lease.next_generation
    });
    generation
}

/// Schedule an off-screen park only if this exact surface is still the active
/// lease. An old surface dropping after a new surface mounts becomes a no-op.
fn request_park(generation: u64) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let should_schedule = SURFACE_LEASE.with(|lease| {
        let mut lease = lease.borrow_mut();
        if lease.active_generation != Some(generation) {
            return false;
        }
        lease.active_generation = None;
        if let Some(timer) = lease.park_timer.take() {
            window.clear_timeout_with_handle(timer);
        }
        true
    });
    if !should_schedule {
        return;
    }

    let cb = Closure::once_into_js(move || {
        let should_park = SURFACE_LEASE.with(|lease| {
            let mut lease = lease.borrow_mut();
            lease.park_timer = None;
            lease.active_generation.is_none() && lease.next_generation == generation
        });
        if should_park {
            park_offscreen();
        }
    });
    if let Ok(id) = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 80)
    {
        SURFACE_LEASE.with(|lease| {
            lease.borrow_mut().park_timer = Some(id);
        });
    }
}

/// The embedded browser surface. `expanded` selects sidebar (false) vs
/// main-area (true) presentation and flips the expand/dock toggle.
#[component]
pub fn BrowserSurface(expanded: bool) -> Element {
    let url = use_signal(|| DEFAULT_URL.to_string());
    let mut url_input = use_signal(|| DEFAULT_URL.to_string());
    let page_title = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut can_go_back = use_signal(|| false);
    let mut can_go_forward = use_signal(|| false);
    let mut browser_error = use_signal(|| None::<String>);
    let load_timeout = use_hook(|| Rc::new(RefCell::new(None::<i32>)));
    let active_load_generation = use_hook(|| Rc::new(Cell::new(0u64)));
    let mut ui_state = use_ui_store();
    let mut panel_state = use_panel_manager_store();
    let mut cleanup: Signal<Option<SurfaceCleanup>> = use_signal(|| None);
    let initialized = use_hook(|| Rc::new(RefCell::new(false)));
    let surface_generation = use_hook(|| Rc::new(RefCell::new(None::<u64>)));
    let browser_unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let mut show_quick_menu = use_signal(|| false);
    let clickaway_listener: Rc<
        RefCell<Option<(web_sys::Window, Closure<dyn FnMut(web_sys::MouseEvent)>)>>,
    > = use_hook(|| Rc::new(RefCell::new(None)));

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
        let surface_generation_for_mount = surface_generation.clone();
        let load_timeout_for_mount = load_timeout.clone();
        let active_generation_for_mount = active_load_generation.clone();
        let browser_unlisteners_for_mount = browser_unlisteners.clone();
        let start_url = url();
        use_effect(move || {
            if *initialized.borrow() {
                return;
            }
            *initialized.borrow_mut() = true;

            // A surface is appearing — acquire a new lease before the
            // predecessor's drop hook can park the shared native WebView.
            let generation = begin_surface();
            *surface_generation_for_mount.borrow_mut() = Some(generation);

            let Some(window) = web_sys::window() else {
                return;
            };

            // Keep native page state synchronized with the toolbar. These
            // listeners are emitted by BrowserManager after Tauri's native
            // child-WebView callbacks observe redirects, clicked links, titles,
            // and load completion.
            let event_id = BROWSER_ID.to_string();
            let mut url_for_event = url;
            let mut input_for_event = url_input;
            let url_listener = tauri_bridge::listen("browser:urlChange", move |payload| {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    return;
                };
                if value.get("id").and_then(|v| v.as_str()) != Some(event_id.as_str()) {
                    return;
                }
                if let Some(next_url) = value.get("url").and_then(|v| v.as_str()) {
                    url_for_event.set(next_url.to_string());
                    input_for_event.set(next_url.to_string());
                }
            });
            if let Ok(unlisten) = url_listener {
                browser_unlisteners_for_mount.borrow_mut().push(unlisten);
            }

            let event_id = BROWSER_ID.to_string();
            let mut title_for_event = page_title;
            let title_listener = tauri_bridge::listen("browser:titleChange", move |payload| {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    return;
                };
                if value.get("id").and_then(|v| v.as_str()) != Some(event_id.as_str()) {
                    return;
                }
                title_for_event.set(
                    value
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                );
            });
            if let Ok(unlisten) = title_listener {
                browser_unlisteners_for_mount.borrow_mut().push(unlisten);
            }

            let event_id = BROWSER_ID.to_string();
            let mut loading_for_status = loading;
            let mut browser_error_for_status = browser_error;
            let load_timeout_for_status = load_timeout_for_mount.clone();
            let active_generation_for_status = active_generation_for_mount.clone();
            let status_listener = tauri_bridge::listen("browser:statusChange", move |payload| {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    return;
                };
                if value.get("id").and_then(|v| v.as_str()) != Some(event_id.as_str()) {
                    return;
                }
                let generation = value
                    .get("generation")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                if generation < active_generation_for_status.get() {
                    return;
                }
                active_generation_for_status.set(generation);
                let status = value.get("status").and_then(|v| v.as_str());
                loading_for_status.set(status == Some("loading"));
                match status {
                    Some("loading") => schedule_load_timeout(
                        load_timeout_for_status.clone(),
                        active_generation_for_status.clone(),
                        generation,
                        loading_for_status,
                        browser_error_for_status,
                    ),
                    Some("failed") => {
                        clear_load_timeout(&load_timeout_for_status);
                        browser_error_for_status.set(Some("The page failed to load.".to_string()));
                    }
                    _ => {
                        clear_load_timeout(&load_timeout_for_status);
                        browser_error_for_status.set(None);
                    }
                }
                can_go_back.set(
                    value
                        .get("canGoBack")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                );
                can_go_forward.set(
                    value
                        .get("canGoForward")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                );
            });
            if let Ok(unlisten) = status_listener {
                browser_unlisteners_for_mount.borrow_mut().push(unlisten);
            }

            // Create (idempotent) the child webview, then position it.
            let create_url = start_url.clone();
            let mut url_after_show = url;
            let mut input_after_show = url_input;
            let mut title_after_show = page_title;
            let mut loading_after_show = loading;
            let mut can_go_back_after_show = can_go_back;
            let mut can_go_forward_after_show = can_go_forward;
            let mut browser_error_after_show = browser_error;
            let active_generation_after_show = active_load_generation.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match tauri_bridge::browser_show(BROWSER_ID, &create_url).await {
                    Ok(value) => {
                        // `browser_show` returns the existing model snapshot
                        // on remount, so docking never resets the toolbar to
                        // the default URL while the native page is preserved.
                        if let Some(snapshot) = value.as_string() {
                            if let Ok(panel) = serde_json::from_str::<serde_json::Value>(&snapshot)
                            {
                                if let Some(next_url) =
                                    panel.get("current_url").and_then(|v| v.as_str())
                                {
                                    url_after_show.set(next_url.to_string());
                                    input_after_show.set(next_url.to_string());
                                }
                                if let Some(title) = panel.get("title").and_then(|v| v.as_str()) {
                                    title_after_show.set(title.to_string());
                                }
                                loading_after_show.set(
                                    panel.get("loading_state").and_then(|v| v.as_str())
                                        == Some("Loading"),
                                );
                                can_go_back_after_show.set(
                                    panel
                                        .get("can_go_back")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false),
                                );
                                can_go_forward_after_show.set(
                                    panel
                                        .get("can_go_forward")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false),
                                );
                                if let Some(generation) =
                                    panel.get("generation").and_then(|v| v.as_u64())
                                {
                                    active_generation_after_show.set(generation);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        let message = format!("Unable to open browser: {error:?}");
                        browser_error_after_show.set(Some(message.clone()));
                        web_sys::console::error_1(&JsValue::from_str(&message));
                    }
                }
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
                        js_sys::Reflect::construct(&c, &js_sys::Array::of1(ro_closure.as_ref()))
                            .ok()
                    });
            if let (Some(observer), Some(document)) = (resize_observer.as_ref(), window.document())
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
                if let Ok(disc) = js_sys::Reflect::get(&observer, &JsValue::from_str("disconnect"))
                {
                    if let Ok(disc) = disc.dyn_into::<js_sys::Function>() {
                        let _ = disc.call0(&observer);
                    }
                }
            }
            if let (Some(window), Some(cl)) = (web_sys::window(), c.window_resize_closure.take()) {
                let _ = window
                    .remove_event_listener_with_callback("resize", cl.as_ref().unchecked_ref());
            }
        }
        for unlisten in browser_unlisteners.borrow_mut().drain(..) {
            unlisten();
        }
        clear_pending_bounds();
        clear_load_timeout(&load_timeout);
        if let Some(generation) = surface_generation.borrow_mut().take() {
            request_park(generation);
        }
    });

    // ── Click-away for the quick-links dropdown ───────────────────────────────
    // Keep the listener in a component-owned slot so toggling the menu removes
    // the previous closure before installing a new one. Cleanup is registered
    // at the component level, never from inside an effect callback.
    {
        let mut menu = show_quick_menu;
        let listener_slot = clickaway_listener.clone();
        use_effect(move || {
            if let Some((window, callback)) = listener_slot.borrow_mut().take() {
                let _ = window.remove_event_listener_with_callback(
                    "mousedown",
                    callback.as_ref().unchecked_ref(),
                );
            }
            if !menu() {
                return;
            }
            let Some(window) = web_sys::window() else {
                return;
            };
            let window_for_cb = window.clone();
            let cb: Closure<dyn FnMut(web_sys::MouseEvent)> =
                Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                    let target = e.target();
                    let node = target.and_then(|t| t.dyn_into::<web_sys::Node>().ok());
                    let outside = match node {
                        Some(node) => match window_for_cb.document() {
                            Some(document) => {
                                let in_popover =
                                    match document.get_element_by_id("browser-quick-menu") {
                                        Some(popover) => popover.contains(Some(&node)),
                                        None => false,
                                    };
                                let in_trigger =
                                    match document.get_element_by_id("quick-links-trigger") {
                                        Some(trigger) => trigger.contains(Some(&node)),
                                        None => false,
                                    };
                                !in_popover && !in_trigger
                            }
                            None => true,
                        },
                        None => true,
                    };
                    if outside {
                        menu.set(false);
                    }
                }));
            let _ =
                window.add_event_listener_with_callback("mousedown", cb.as_ref().unchecked_ref());
            *listener_slot.borrow_mut() = Some((window, cb));
        });
    }
    {
        let listener_slot = clickaway_listener.clone();
        use_drop(move || {
            if let Some((window, callback)) = listener_slot.borrow_mut().take() {
                let _ = window.remove_event_listener_with_callback(
                    "mousedown",
                    callback.as_ref().unchecked_ref(),
                );
            }
        });
    }

    let toggle_title = if expanded {
        "Dock to sidebar"
    } else {
        "Expand to main area"
    };

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: 0; overflow: hidden;",

            // ── Toolbar ──────────────────────────────────────────────────────
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                button {
                    class: "icon-btn lit-sweep",
                    title: "Back",
                    disabled: !can_go_back(),
                    onclick: move |_| {
                        let mut url_clone = url;
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_back(BROWSER_ID).await {
                                Ok(new_url) => {
                                    browser_error.set(None);
                                    url_clone.set(new_url.clone());
                                    loading.set(true);
                                },
                                Err(e) => {
                                    let message = format!("Back navigation failed: {e:?}");
                                    browser_error.set(Some(message.clone()));
                                    web_sys::console::warn_1(&JsValue::from_str(&message));
                                },
                            }
                        });
                    },
                    IconArrowLeft { size: Some(16), color: Some("currentColor".to_string()) }
                }

                button {
                    class: "icon-btn lit-sweep",
                    title: "Forward",
                    disabled: !can_go_forward(),
                    onclick: move |_| {
                        let mut url_clone = url;
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_forward(BROWSER_ID).await {
                                Ok(new_url) => {
                                    browser_error.set(None);
                                    url_clone.set(new_url.clone());
                                    loading.set(true);
                                },
                                Err(e) => {
                                    let message = format!("Forward navigation failed: {e:?}");
                                    browser_error.set(Some(message.clone()));
                                    web_sys::console::warn_1(&JsValue::from_str(&message));
                                },
                            }
                        });
                    },
                    IconArrowRight { size: Some(16), color: Some("currentColor".to_string()) }
                }

                button {
                    class: "icon-btn lit-sweep",
                    title: "Reload",
                    onclick: move |_| {
                        loading.set(true);
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_reload(BROWSER_ID).await {
                                Ok(_) => browser_error.set(None),
                                Err(error) => {
                                    let message = format!("Reload failed: {error:?}");
                                    browser_error.set(Some(message.clone()));
                                    web_sys::console::warn_1(&JsValue::from_str(&message));
                                }
                            }
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
                                            browser_error.set(None);
                                            url_clone.set(trimmed.clone());
                                            input_clone.set(trimmed);
                                            loading.set(true);
                                        }
                                        Err(e) => {
                                            let message = format!("Navigation failed: {e:?}");
                                            browser_error.set(Some(message.clone()));
                                            web_sys::console::error_1(&JsValue::from_str(&message));
                                        }
                                    }
                                });
                            }
                        }
                    },
                    placeholder: "Enter URL or search..."
                }

                if loading() {
                    span {
                        style: "font-size: 10px; color: var(--accent); white-space: nowrap;",
                        "Loading…"
                    }
                } else if !page_title().is_empty() {
                    span {
                        style: "max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 10px; color: var(--textMuted);",
                        "{page_title}"
                    }
                }

                // Expand / dock toggle
                button {
                    class: "icon-btn lit-sweep",
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

            if let Some(error) = browser_error() {
                div {
                    style: "padding: 6px 10px; border-bottom: 1px solid var(--border); color: var(--danger, #d66); font-size: 11px;",
                    "{error}"
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
                    style: "font-family: var(--font-display); font-size: 16px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "Loading browser…"
                }
                div {
                    style: "font-size: 11px; max-width: 280px; color: var(--textMuted);",
                    "Web content renders in a native view over this area. Use the URL bar or the shortcuts below."
                }
            }

            // ── Quick links (collapsed dropdown) ─────────────────────────────
            // Quick Access + Localhost entries collapsed into a single button
            // that opens a popover menu, restoring viewport height.
            div {
                style: "position: relative; border-top: 1px solid var(--border); padding: 8px 12px; flex-shrink: 0;",

                button {
                    id: "quick-links-trigger",
                    class: "icon-btn lit-sweep",
                    style: "width: 100%; justify-content: space-between; padding: 5px 10px; font-size: 12px; color: var(--text);",
                    title: "Quick links",
                    onclick: move |_| show_quick_menu.toggle(),
                    span {
                        style: "display: flex; align-items: center; gap: 6px;",
                        IconGlobe { size: Some(13), color: Some("currentColor".to_string()) }
                        "Quick links"
                    }
                    IconChevronDown {
                        size: Some(13),
                        color: Some("currentColor".to_string()),
                        // flip the chevron when open for a touch of motion
                        // (no extra deps — style-only transform)
                    }
                }

                if show_quick_menu() {
                    div {
                        id: "browser-quick-menu",
                        class: "quick-menu",
                        style: "position: absolute; left: 8px; right: 8px; bottom: calc(100% + 4px);",
                        div {
                            class: "quick-menu-section",
                            "Quick Access"
                        }
                        for (name, url_str) in quick_urls.iter().cloned() {
                            button {
                                class: "quick-menu-row lit-sweep",
                                onclick: move |_| {
                                    show_quick_menu.set(false);
                                    let target = url_str.to_string();
                                    let mut url_clone = url;
                                    let mut input_clone = url_input;
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match tauri_bridge::browser_navigate(BROWSER_ID, &target).await {
                                            Ok(_) => {
                                                url_clone.set(target.clone());
                                                input_clone.set(target);
                                                loading.set(true);
                                            }
                                            Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e))),
                                        }
                                    });
                                },
                                "{name}"
                            }
                        }
                        div {
                            class: "quick-menu-section",
                            "Localhost"
                        }
                        for (label, url_str) in localhost_urls.iter().cloned() {
                            button {
                                class: "quick-menu-row lit-sweep",
                                onclick: move |_| {
                                    show_quick_menu.set(false);
                                    let target = url_str.to_string();
                                    let mut url_clone = url;
                                    let mut input_clone = url_input;
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match tauri_bridge::browser_navigate(BROWSER_ID, &target).await {
                                            Ok(_) => {
                                                url_clone.set(target.clone());
                                                input_clone.set(target);
                                                loading.set(true);
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
