use crate::stores::terminal::use_terminal_store;
use crate::stores::terminal::TerminalSession;
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{use_workspace_store, AgentType};
use crate::tauri_bridge::{
    pty_default_shell_cached, pty_has_session, pty_listen_raw, pty_resize, pty_set_xterm,
    pty_spawn, pty_write, read_clipboard_text,
};
use crate::utils::resume_scanner::ResumeScanner;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Default scrollback buffer size for xterm.js sessions.
/// Previously hardcoded to 2500; raised to 10000 to
/// accommodate long-running build/logs without truncation.
const XTERM_SCROLLBACK: f64 = 10000.0;

/// Holds the resources needed to tear down an xterm.js mount on unmount.
struct XtermCleanup {
    /// The Terminal instance; calling .dispose() releases internal
    /// DOM nodes, WebGL contexts, and event listeners.
    term: JsValue,
    /// Unlisten function for the `pty:raw` event subscription.
    unlisten: Option<Box<dyn FnOnce()>>,
    /// Rooted `onData` closure. Prevents duplicate terminal creation on re-renders.
    _on_data_closure: JsValue,
    /// Rooted `onResize` closure. Dropping this JsValue lets the JS GC
    /// reclaim the closure once `term.dispose()` has detached it.
    _on_resize_closure: JsValue,
    /// ResizeObserver that calls `fit()` when the container resizes.
    /// Disconnected on unmount to release the callback closure.
    _resize_observer: Option<JsValue>,
    /// Rooted ResizeObserver callback closure. Kept alive while the observer
    /// is connected; dropped on unmount to avoid leaking the closure.
    _ro_closure: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
    /// Rooted keydown handler for custom macOS keyboard shortcuts.
    _keydown_handler: Option<JsValue>,
    /// IntersectionObserver that detects when the terminal container
    /// becomes visible after being hidden (e.g. display:none toggle).
    _visibility_observer: Option<JsValue>,
    /// Rooted IntersectionObserver callback closure.
    _vis_callback: Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>>,
}

/// Mount an xterm.js Terminal into a div with id `pane_id`.
///
/// Subscribes to the raw PTY output stream for the pane's session and
/// pipes incoming bytes directly into the terminal.
#[component]
pub fn XtermMount(
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    resume_id: Option<String>,
    custom_cmd: Option<String>,
) -> Element {
    let mount_id = pane_id.clone();
    let mut cleanup: Signal<Option<XtermCleanup>> = use_signal(|| None);
    let is_initialized = use_hook(|| Rc::new(RefCell::new(false)));
    let term_ref: Signal<Option<JsValue>> = use_signal(|| None);
    let mut terminal_store = use_terminal_store();
    let workspace_store = use_workspace_store();
    let ui_state = use_ui_store();

    use_effect(move || {
        if *is_initialized.borrow() {
            return;
        }

        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(container) = document.get_element_by_id(&mount_id) else {
            web_sys::console::error_1(
                &format!("XtermMount: container #{mount_id} not found").into(),
            );
            return;
        };

        // Guard: if container already has .xterm children, clear stale DOM first
        if let Some(_existing) = container.query_selector(".xterm").ok().flatten() {
            web_sys::console::warn_1(
                &format!(
                    "[XtermMount] Found existing .xterm in #{}; clearing stale DOM before open",
                    mount_id
                )
                .into(),
            );
            container.set_inner_html(""); // clear any stale xterm DOM
        }

        *is_initialized.borrow_mut() = true;

        let agent_type_for_spawn = agent_type.clone();
        let resume_id_for_spawn = resume_id.clone();
        let custom_cmd_for_spawn = custom_cmd.clone();
        let mount_id_for_spawn = mount_id.clone();
        let spawn_cwd = if cwd.trim().is_empty() {
            "/tmp".to_string()
        } else {
            cwd.clone()
        };
        let mut cleanup = cleanup.clone();
        let mut term_ref = term_ref.clone();
        let window = window.clone();
        let container = container.clone();

        // Ensure a PTY session exists before initializing xterm, then set up
        // the terminal. Everything that touches the xterm instance happens in
        // this async block so we can `await` the backend spawn first.
        spawn(async move {
            let mut store = use_terminal_store();
            let has_session = {
                let s = store.read();
                s.sessions.contains_key(&mount_id_for_spawn)
            };
            let has_backend = pty_has_session(&mount_id_for_spawn).await.unwrap_or(false);
            let reusing_existing_session = has_session || has_backend;
            if !has_session {
                if !has_backend {
                    let shell = pty_default_shell_cached().await;
                    web_sys::console::log_1(
                        &format!(
                            "[XtermMount] spawning PTY id={} cwd={} shell={} cols=80 rows=24",
                            mount_id_for_spawn, spawn_cwd, shell
                        )
                        .into(),
                    );
                    if let Err(e) = pty_spawn(&mount_id_for_spawn, &spawn_cwd, &shell, 80, 24).await
                    {
                        web_sys::console::error_1(
                            &format!(
                                "XtermMount: pty_spawn failed for id={} cwd={} shell={}: {e:?}",
                                mount_id_for_spawn, spawn_cwd, shell
                            )
                            .into(),
                        );
                        return;
                    }
                    web_sys::console::log_1(
                        &format!(
                            "[XtermMount] PTY spawn succeeded for id={}",
                            mount_id_for_spawn
                        )
                        .into(),
                    );
                }
                store.write().ensure_session(&mount_id_for_spawn, 80, 24);

                // NOTE: We intentionally do NOT auto-run `claude --resume <id>`
                // here. A stored `resume_id` is surfaced to the user as a
                // dismissible banner in the pane header (see PaneItem in
                // terminal_grid.rs); the user chooses when to resume. This keeps
                // the user in control and avoids re-running a session on every
                // app launch. The `resume_id_for_spawn` binding is retained only
                // so the spawn signature stays stable for custom_cmd handling.
                let _ = &resume_id_for_spawn;
                if !has_backend {
                    // Write custom agent command into newly spawned shell
                    if let Some(ref cmd_str) = custom_cmd_for_spawn {
                        if matches!(agent_type_for_spawn, AgentType::Custom) {
                            let cmd_with_newline = format!("{}\n", cmd_str);
                            let mount_id_for_custom = mount_id_for_spawn.clone();
                            spawn(async move {
                                if let Err(e) =
                                    pty_write(&mount_id_for_custom, &cmd_with_newline).await
                                {
                                    web_sys::console::error_1(
                                        &format!(
                                            "XtermMount: custom command write failed: {:?}",
                                            e
                                        )
                                        .into(),
                                    );
                                }
                            });
                        }
                    }
                }
            } else {
                web_sys::console::log_1(
                    &format!(
                        "[XtermMount] reusing existing PTY session id={}",
                        mount_id_for_spawn
                    )
                    .into(),
                );
            }

            let mount_id = mount_id_for_spawn;
            store.write().set_session_xterm(&mount_id, true);

            // Tell the backend this session is xterm-managed so it can skip
            // emitting `terminal:data` cell-delta events (xterm.js parses
            // raw ANSI bytes itself).
            let _ = pty_set_xterm(&mount_id, true).await;

            let term_ctor_val = js_sys::Reflect::get(&window, &JsValue::from_str("Terminal"))
                .unwrap_or(JsValue::UNDEFINED);
            let Ok(term_ctor) = term_ctor_val.dyn_into::<js_sys::Function>() else {
                web_sys::console::error_1(&"XtermMount: window.Terminal not loaded".into());
                return;
            };

            let bg = read_css_var(&window, "--terminalBg");
            let fg = read_css_var(&window, "--terminalFg");
            let cursor = read_css_var(&window, "--terminalCursor");
            let selection = read_css_var(&window, "--terminalSelection");

            let theme = js_sys::Object::new();
            if !bg.is_empty() {
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("background"),
                    &JsValue::from_str(&bg),
                );
            }
            if !fg.is_empty() {
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("foreground"),
                    &JsValue::from_str(&fg),
                );
            }
            if !cursor.is_empty() {
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("cursor"),
                    &JsValue::from_str(&cursor),
                );
            }
            if !selection.is_empty() {
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("selectionBackground"),
                    &JsValue::from_str(&selection),
                );
            }

            let options = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("fontFamily"),
                &JsValue::from_str("'JetBrains Mono', 'Cascadia Code', monospace"),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("fontSize"),
                &JsValue::from_f64(14.0),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("cursorBlink"),
                &JsValue::from_bool(true),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("convertEol"),
                &JsValue::from_bool(true),
            );
            let _ = js_sys::Reflect::set(&options, &JsValue::from_str("theme"), &theme);
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("scrollback"),
                &JsValue::from_f64(XTERM_SCROLLBACK),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("allowProposedApi"),
                &JsValue::from_bool(true),
            );

            let term_val =
                match js_sys::Reflect::construct(&term_ctor, &js_sys::Array::of1(&options)) {
                    Ok(t) => t,
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("XtermMount: new Terminal() failed: {e:?}").into(),
                        );
                        return;
                    }
                };

            let open_fn_val = js_sys::Reflect::get(&term_val, &JsValue::from_str("open"))
                .unwrap_or(JsValue::UNDEFINED);
            let Ok(open_fn) = open_fn_val.dyn_into::<js_sys::Function>() else {
                web_sys::console::error_1(&"XtermMount: term.open is not a function".into());
                return;
            };
            if let Err(e) = open_fn.call1(&term_val, container.as_ref()) {
                web_sys::console::error_1(&format!("XtermMount: term.open() failed: {e:?}").into());
                return;
            }
            web_sys::console::log_1(
                &format!("[XtermMount] terminal opened for id={}", mount_id).into(),
            );

            // ── Custom keyboard shortcuts (macOS) ────────────────────────────
            // xterm.js does not send distinct sequences for Shift+Enter or
            // Cmd+Delete.  We intercept them in capture phase, write the
            // appropriate escape sequence to the PTY, and prevent xterm.js
            // from also forwarding its default sequence.
            let pane_id_keydown = mount_id.clone();
            let keydown_handler = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                let is_mac = web_sys::window()
                    .and_then(|w| w.navigator().platform().ok())
                    .map(|p| {
                        let p = p.to_lowercase();
                        p.contains("mac") || p.contains("darwin")
                    })
                    .unwrap_or(false);

                let meta = event.meta_key();
                let shift = event.shift_key();
                let ctrl = event.ctrl_key();
                let alt = event.alt_key();
                let key = event.key();

                // Cmd+V (macOS) / Ctrl+V (others) → paste via Tauri clipboard with bracketed paste sequences
                if key == "v" && ((is_mac && meta && !ctrl) || (!is_mac && ctrl && !meta)) && !shift && !alt {
                    event.prevent_default();
                    event.stop_propagation();
                    let pane_id = pane_id_keydown.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match read_clipboard_text().await {
                            Ok(text) => {
                                let bracketed = format!("\x1b[200~{}\x1b[201~", text);
                                if let Err(e) = pty_write(&pane_id, &bracketed).await {
                                    web_sys::console::error_1(
                                        &format!("XtermMount: bracketed paste write failed: {:?}", e).into(),
                                    );
                                }
                            }
                            Err(e) => {
                                web_sys::console::error_1(
                                    &format!("XtermMount: read_clipboard_text failed: {:?}", e).into(),
                                );
                            }
                        }
                    });
                    return;
                }

                if !is_mac {
                    return;
                }

                // Shift+Enter → literal newline (not execute)
                if key == "Enter" && shift && !meta && !ctrl && !alt {
                    event.prevent_default();
                    event.stop_propagation();
                    let pane_id = pane_id_keydown.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = pty_write(&pane_id, "\n").await;
                    });
                    return;
                }

                // Cmd+Delete (Backspace) → delete to beginning of line
                // Maps to readline's unix-line-discard (Ctrl+U).
                if key == "Backspace" && meta && !shift && !ctrl && !alt {
                    event.prevent_default();
                    event.stop_propagation();
                    let pane_id = pane_id_keydown.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = pty_write(&pane_id, "\x15").await;
                    });
                    return;
                }
            })
                as Box<dyn FnMut(web_sys::KeyboardEvent)>);
            let keydown_handler_js = keydown_handler.into_js_value();
            let _ = container.add_event_listener_with_callback_and_bool(
                "keydown",
                keydown_handler_js.as_ref().unchecked_ref(),
                true,
            );

            // On a brand new PTY, clear once before the first fit-triggered
            // resize to avoid a duplicated initial prompt. When reusing an
            // existing PTY session, do not clear; instead, we restore the
            // current screen contents from the frontend store below.
            if !reusing_existing_session {
                if let Ok(clear_val) = js_sys::Reflect::get(&term_val, &JsValue::from_str("clear"))
                {
                    if let Ok(clear_fn) = clear_val.dyn_into::<js_sys::Function>() {
                        let _ = clear_fn.call0(&term_val);
                    }
                }
            }

            // Store terminal reference for focus on click
            term_ref.set(Some(term_val.clone()));

            // ── Write Coalescing — accumulate PTY bytes and flush on rAF ─────
            let wq_term = term_val.clone();
            let wq_queue: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
            let wq_scheduled: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
            let wq_flush = wq_queue.clone();
            let wq_sched = wq_scheduled.clone();

            let mount_id_for_scan = mount_id.clone();
            let workspace_for_scan = workspace_store.clone();
            let mut resume_scanner = ResumeScanner::new();
            let unlisten = match pty_listen_raw(&mount_id, move |bytes: Vec<u8>| {
                let text = String::from_utf8_lossy(&bytes).to_string();
                wq_queue.borrow_mut().push(bytes);
                if !*wq_scheduled.borrow() {
                    *wq_scheduled.borrow_mut() = true;
                    let q = wq_flush.clone();
                    let s = wq_sched.clone();
                    let t = wq_term.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let q_for_closure = q.clone();
                        let s_for_closure = s.clone();
                        let t_for_closure = t.clone();
                        let closure = Closure::once_into_js(Box::new(move || {
                            let chunks = q_for_closure.borrow_mut().drain(..).collect::<Vec<_>>();
                            for chunk in chunks {
                                write_bytes_to_term(&t_for_closure, &chunk);
                            }
                            *s_for_closure.borrow_mut() = false;
                        }) as Box<dyn FnOnce()>);

                        let mut raf_failed = true;
                        if let Some(window) = web_sys::window() {
                            if window.request_animation_frame(closure.as_ref().unchecked_ref()).is_ok() {
                                raf_failed = false;
                            }
                        }
                        if raf_failed {
                            let chunks = q.borrow_mut().drain(..).collect::<Vec<_>>();
                            for chunk in chunks {
                                write_bytes_to_term(&t, &chunk);
                            }
                            *s.borrow_mut() = false;
                        }
                    });
                }
                if let Some((prefix, id)) = resume_scanner.feed(&text) {
                    // Reconstruct the full command for Shell→manual-claude case
                    let full_cmd = format!("{} {};", prefix, &id);
                    web_sys::console::log_1(
                        &format!(
                            "[XtermMount] capture resume id={} cmd={} for pane={}",
                            id, full_cmd, mount_id_for_scan
                        )
                        .into(),
                    );
                    let mid = mount_id_for_scan.clone();
                    let mut ws = workspace_for_scan.clone();
                    let cmd_for_store = full_cmd.clone();
                    // IMPORTANT: use wasm_bindgen_futures::spawn_local here, not
                    // Dioxus's `spawn`. This closure runs inside a raw
                    // pty_listen_raw JS callback — outside any Dioxus scope — so
                    // Dioxus's `spawn` panics at current_scope_id().unwrap()
                    // (scope stack is empty). spawn_local does not need a scope.
                    wasm_bindgen_futures::spawn_local(async move {
                        let mut space_id: Option<String> = None;
                        {
                            let ws_guard = ws.read();
                            for space in &ws_guard.spaces {
                                if space.panes.iter().any(|p| p.id == mid) {
                                    space_id = Some(space.id.clone());
                                    break;
                                }
                            }
                        }
                        if let Some(sid) = space_id {
                            ws.write().update_space(&sid, |space| {
                                for pane in &mut space.panes {
                                    if pane.id == mid {
                                        pane.resume_id = Some(id.clone());
                                        pane.resume_cmd = Some(cmd_for_store.clone());
                                        // A new session supersedes any previously
                                        // dismissed banner: reset so the resume
                                        // banner reappears for this new id.
                                        pane.resume_dismissed = Some(false);
                                        web_sys::console::log_1(
                                            &format!(
                                                "[XtermMount] persisted resume_id for pane={}",
                                                mid
                                            )
                                            .into(),
                                        );
                                        break;
                                    }
                                }
                            });
                        }
                    });
                }
            }) {
                Ok(u) => u,
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("XtermMount: pty_listen_raw failed: {e:?}").into(),
                    );
                    return;
                }
            };

            if reusing_existing_session {
                let existing_session = {
                    let s = store.read();
                    s.sessions.get(&mount_id).cloned()
                };
                if let Some(session) = existing_session.as_ref() {
                    restore_term_from_session(&term_val, session);
                }
            }

            let pane_id_for_data = mount_id.clone();
            let on_data_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |data: String| {
                    let pane_id = pane_id_for_data.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Err(e) = pty_write(&pane_id, &data).await {
                            web_sys::console::error_1(
                                &format!("XtermMount: pty_write failed: {:?}", e).into(),
                            );
                        }
                    });
                }) as Box<dyn FnMut(String)>);
            let on_data_closure_js = on_data_closure.into_js_value();
            if let Some(on_data_fn) = js_sys::Reflect::get(&term_val, &JsValue::from_str("onData"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
            {
                let _ = on_data_fn.call1(&term_val, on_data_closure_js.as_ref());
            }

            let pane_id_for_resize = mount_id.clone();
            let on_resize_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |event: JsValue| {
                    let cols = js_sys::Reflect::get(&event, &JsValue::from_str("cols"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .map(|n| n as u16)
                        .unwrap_or(80);
                    let rows = js_sys::Reflect::get(&event, &JsValue::from_str("rows"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .map(|n| n as u16)
                        .unwrap_or(24);
                    let pane_id = pane_id_for_resize.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = pty_resize(&pane_id, cols, rows).await;
                    });
                }) as Box<dyn FnMut(JsValue)>);
            let on_resize_closure_js = on_resize_closure.into_js_value();
            if let Some(on_resize_fn) =
                js_sys::Reflect::get(&term_val, &JsValue::from_str("onResize"))
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
            {
                let _ = on_resize_fn.call1(&term_val, on_resize_closure_js.as_ref());
            }

            let _ = try_activate_addon(&window, "CanvasAddon", &term_val);

            let mut resize_observer_holder: Option<JsValue> = None;
            let mut ro_closure_holder: Option<wasm_bindgen::closure::Closure<dyn FnMut()>> = None;
            if let Some(fit_instance) = try_activate_addon(&window, "FitAddon", &term_val) {
                // Initial fit so the terminal has correct cols/rows before any data arrives.
                schedule_fit(&window, &fit_instance, &container);

                let fit_for_ro = fit_instance.clone();
                let container_for_ro = container.clone();
                let ro_timer: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
                let ro_timer_for_cb = ro_timer.clone();
                let ro_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    if let Some(w) = web_sys::window() {
                        if let Some(id) = ro_timer_for_cb.borrow_mut().take() {
                            let _ = w.clear_timeout_with_handle(id);
                        }
                        let fit_for_cb = fit_for_ro.clone();
                        let container_for_cb = container_for_ro.clone();
                        // Single-fire timer closure — once_into_js auto-frees
                        // after the timeout fires, so this no longer leaks one
                        // Closure per resize tick.
                        let timer = wasm_bindgen::closure::Closure::once_into_js(move || {
                            if let Some(win) = web_sys::window() {
                                schedule_fit(&win, &fit_for_cb, &container_for_cb);
                            }
                        });
                        let handle = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                            timer.as_ref().unchecked_ref(),
                            50,
                        );
                        if let Ok(h) = handle {
                            *ro_timer_for_cb.borrow_mut() = Some(h);
                        }
                    }
                })
                    as Box<dyn FnMut()>);
                let resize_observer =
                    js_sys::Reflect::get(&window, &JsValue::from_str("ResizeObserver"))
                        .ok()
                        .and_then(|ctor| ctor.dyn_into::<js_sys::Function>().ok())
                        .and_then(|ctor| {
                            js_sys::Reflect::construct(
                                &ctor,
                                &js_sys::Array::of1(ro_closure.as_ref()),
                            )
                            .ok()
                        });
                match resize_observer {
                    Some(observer) => {
                        if let Ok(observe_fn) =
                            js_sys::Reflect::get(&observer, &JsValue::from_str("observe"))
                        {
                            if let Ok(observe_fn) = observe_fn.dyn_into::<js_sys::Function>() {
                                let _ = observe_fn.call1(&observer, container.as_ref());
                            }
                        }
                        ro_closure_holder = Some(ro_closure);
                        resize_observer_holder = Some(observer);
                    }
                    None => {
                        // ro_closure dropped naturally when it goes out of scope
                    }
                }
                // NOTE: removed the 75ms setInterval fallback. ResizeObserver
                // handles all shape changes; the interval caused jitter by
                // calling fit() during scroll / continuous output.
            }
            // ── IntersectionObserver: redraw when container becomes visible ──
            // When the terminal is hidden inside a display:none subtree (e.g. panel
            // switch), the browser discards the <canvas> backing store, leaving the
            // terminal blank. Detect visibility restoration and force a full redraw.
            let term_for_vis = term_val.clone();
            let term_for_vis_fit = term_val.clone();
            let vis_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |entries: JsValue| {
                    let Ok(arr) = entries.dyn_into::<js_sys::Array>() else {
                        return;
                    };
                    for entry in arr.iter() {
                        let is_intersecting =
                            js_sys::Reflect::get(&entry, &JsValue::from_str("isIntersecting"))
                                .ok()
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                        if is_intersecting {
                            let rows =
                                js_sys::Reflect::get(&term_for_vis, &JsValue::from_str("rows"))
                                    .ok()
                                    .and_then(|v| v.as_f64())
                                    .map(|v| v as i32)
                                    .unwrap_or(24);
                            if rows > 0 {
                                if let Ok(refresh_val) = js_sys::Reflect::get(
                                    &term_for_vis,
                                    &JsValue::from_str("refresh"),
                                ) {
                                    if let Ok(refresh_fn) =
                                        refresh_val.dyn_into::<js_sys::Function>()
                                    {
                                        let _ = refresh_fn.call2(
                                            &term_for_vis,
                                            &JsValue::from_f64(0.0),
                                            &JsValue::from_f64((rows - 1) as f64),
                                        );
                                    }
                                }
                            }
                            // Also refit after becoming visible again:
                            let fit_ref = term_for_vis_fit.clone();
                            let _ = web_sys::window().and_then(|w| {
                                w.set_timeout_with_callback_and_timeout_and_arguments_0(
                                    Closure::once_into_js(Box::new(move || {
                                        if let Ok(fit_val) = js_sys::Reflect::get(
                                            &fit_ref,
                                            &JsValue::from_str("fit"),
                                        ) {
                                            if let Ok(fit_fn) =
                                                fit_val.dyn_into::<js_sys::Function>()
                                            {
                                                let _ = fit_fn.call0(&fit_ref);
                                            }
                                        }
                                    }))
                                    .unchecked_ref(),
                                    50,
                                )
                                .ok()
                            });
                            break;
                        }
                    }
                }) as Box<dyn FnMut(JsValue)>);
            let vis_observer =
                js_sys::Reflect::get(&window, &JsValue::from_str("IntersectionObserver"))
                    .ok()
                    .and_then(|ctor| ctor.dyn_into::<js_sys::Function>().ok())
                    .and_then(|ctor| {
                        js_sys::Reflect::construct(&ctor, &js_sys::Array::of1(vis_closure.as_ref()))
                            .ok()
                    });
            let mut vis_observer_holder: Option<JsValue> = None;
            let mut vis_callback_holder: Option<
                wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
            > = None;
            if let Some(ref observer) = vis_observer {
                if let Ok(observe_fn) =
                    js_sys::Reflect::get(observer, &JsValue::from_str("observe"))
                {
                    if let Ok(observe_fn) = observe_fn.dyn_into::<js_sys::Function>() {
                        let _ = observe_fn.call1(observer, container.as_ref());
                    }
                }
                vis_observer_holder = Some(observer.clone());
                vis_callback_holder = Some(vis_closure);
            }

            cleanup.set(Some(XtermCleanup {
                term: term_val,
                unlisten: Some(unlisten),
                _on_data_closure: on_data_closure_js,
                _on_resize_closure: on_resize_closure_js,
                _resize_observer: resize_observer_holder,
                _ro_closure: ro_closure_holder,
                _keydown_handler: Some(keydown_handler_js),
                _visibility_observer: vis_observer_holder,
                _vis_callback: vis_callback_holder,
            }));
        });
    });

    // ── Reactive theme update ────────────────────────────────────────────────
    // When the app theme changes, re-read the CSS variables and push them
    // into the already-instantiated xterm.js terminal.
    let term_ref_for_theme = term_ref.clone();
    use_effect(move || {
        let _theme = ui_state.read().theme;
        let term_opt = term_ref_for_theme();

        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(term) = term_opt else {
            return;
        };

        let bg = read_css_var(&window, "--terminalBg");
        let fg = read_css_var(&window, "--terminalFg");
        let cursor = read_css_var(&window, "--terminalCursor");
        let selection = read_css_var(&window, "--terminalSelection");

        let new_theme = js_sys::Object::new();
        if !bg.is_empty() {
            let _ = js_sys::Reflect::set(
                &new_theme,
                &JsValue::from_str("background"),
                &JsValue::from_str(&bg),
            );
        }
        if !fg.is_empty() {
            let _ = js_sys::Reflect::set(
                &new_theme,
                &JsValue::from_str("foreground"),
                &JsValue::from_str(&fg),
            );
        }
        if !cursor.is_empty() {
            let _ = js_sys::Reflect::set(
                &new_theme,
                &JsValue::from_str("cursor"),
                &JsValue::from_str(&cursor),
            );
        }
        if !selection.is_empty() {
            let _ = js_sys::Reflect::set(
                &new_theme,
                &JsValue::from_str("selectionBackground"),
                &JsValue::from_str(&selection),
            );
        }

        if let Ok(options) = js_sys::Reflect::get(&term, &JsValue::from_str("options")) {
            let _ = js_sys::Reflect::set(&options, &JsValue::from_str("theme"), &new_theme);
        }
    });

    use_drop(move || {
        if let Some(mut c) = cleanup.take() {
            if let Some(unlisten) = c.unlisten.take() {
                unlisten();
            }
            if let Some(observer) = c._resize_observer.take() {
                if let Ok(disconnect_val) =
                    js_sys::Reflect::get(&observer, &JsValue::from_str("disconnect"))
                {
                    if let Ok(disconnect_fn) = disconnect_val.dyn_into::<js_sys::Function>() {
                        let _ = disconnect_fn.call0(&observer);
                    }
                }
            }
            if let Some(observer) = c._visibility_observer.take() {
                if let Ok(disconnect_val) =
                    js_sys::Reflect::get(&observer, &JsValue::from_str("disconnect"))
                {
                    if let Ok(disconnect_fn) = disconnect_val.dyn_into::<js_sys::Function>() {
                        let _ = disconnect_fn.call0(&observer);
                    }
                }
            }
            if let Ok(dispose_val) = js_sys::Reflect::get(&c.term, &JsValue::from_str("dispose")) {
                if let Ok(dispose_fn) = dispose_val.dyn_into::<js_sys::Function>() {
                    let _ = dispose_fn.call0(&c.term);
                }
            }
        }
    });

    rsx! {
        div {
            id: "{pane_id}",
            class: "xterm-mount",
            style: "width: 100%; height: 100%; min-height: 0; flex: 1; background: var(--bg); position: relative; overflow: hidden; padding-left: 4px; padding-bottom: 4px; box-sizing: border-box;",
            onpointerdown: move |e| {
                e.stop_propagation();
                terminal_store.write().set_active(pane_id.clone());
                if let Some(term) = term_ref() {
                    // Focus the xterm.js instance.
                    if let Ok(focus_val) = js_sys::Reflect::get(&term, &JsValue::from_str("focus")) {
                        if let Ok(focus_fn) = focus_val.dyn_into::<js_sys::Function>() {
                            if let Err(e) = focus_fn.call0(&term) {
                                web_sys::console::warn_1(&format!("XtermMount: focus() failed: {e:?}").into());
                            }
                        }
                    }
                }
            },
        }
    }
}

fn read_css_var(window: &web_sys::Window, name: &str) -> String {
    let Some(doc_el) = window.document().and_then(|d| d.document_element()) else {
        return String::new();
    };
    let computed_val = js_sys::Reflect::get(window, &JsValue::from_str("getComputedStyle"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call1(window, &doc_el).ok());
    let Some(computed_val) = computed_val else {
        return String::new();
    };
    js_sys::Reflect::get(&computed_val, &JsValue::from_str("getPropertyValue"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call1(&computed_val, &JsValue::from_str(name)).ok())
        .and_then(|v| v.as_string())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn try_activate_addon(
    window: &web_sys::Window,
    global_name: &str,
    term_val: &JsValue,
) -> Option<JsValue> {
    let global_val = js_sys::Reflect::get(window, &JsValue::from_str(global_name)).ok()?;
    let ctor_val = if global_val.is_function() {
        global_val
    } else {
        js_sys::Reflect::get(&global_val, &JsValue::from_str(global_name)).ok()?
    };
    if !ctor_val.is_function() {
        return None;
    }
    let ctor: js_sys::Function = ctor_val.dyn_into().ok()?;
    let instance = js_sys::Reflect::construct(&ctor, &js_sys::Array::new()).ok()?;
    let activate_val = js_sys::Reflect::get(&instance, &JsValue::from_str("activate")).ok()?;
    let activate_fn: js_sys::Function = activate_val.dyn_into().ok()?;
    let _ = activate_fn.call1(&instance, term_val);
    Some(instance)
}

fn write_bytes_to_term(term_val: &JsValue, bytes: &[u8]) {
    let Ok(write_val) = js_sys::Reflect::get(term_val, &JsValue::from_str("write")) else {
        return;
    };
    let Ok(write_fn) = write_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let bytes_arr = js_sys::Uint8Array::from(bytes);
    let _ = write_fn.call1(term_val, bytes_arr.as_ref());
}

fn write_str_to_term(term_val: &JsValue, text: &str) {
    let Ok(write_val) = js_sys::Reflect::get(term_val, &JsValue::from_str("write")) else {
        return;
    };
    let Ok(write_fn) = write_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = write_fn.call1(term_val, &JsValue::from_str(text));
}

fn restore_term_from_session(term_val: &JsValue, session: &TerminalSession) {
    let mut snapshot = String::from("\u{1b}[2J\u{1b}[H");

    for (row_idx, row) in session.grid.iter().enumerate() {
        let mut line = String::new();
        for cell in row.iter() {
            line.push_str(&cell.text);
        }
        while line.ends_with(' ') {
            line.pop();
        }
        snapshot.push_str(&line);
        if row_idx + 1 < session.grid.len() {
            snapshot.push_str("\r\n");
        }
    }

    let cursor_row = session.cursor_y.saturating_add(1);
    let cursor_col = session.cursor_x.saturating_add(1);
    snapshot.push_str(&format!("\u{1b}[{};{}H", cursor_row, cursor_col));

    write_str_to_term(term_val, &snapshot);
}

fn call_fit(fit_instance: &JsValue, container: &web_sys::Element) {
    let rect = container.get_bounding_client_rect();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let Ok(fit_val) = js_sys::Reflect::get(fit_instance, &JsValue::from_str("fit")) else {
        return;
    };
    let Ok(fit_fn) = fit_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = fit_fn.call0(fit_instance);
}

fn schedule_fit(window: &web_sys::Window, fit_instance: &JsValue, container: &web_sys::Element) {
    let fit_for_raf = fit_instance.clone();
    let container_for_raf = container.clone();
    // RAF callbacks fire exactly once, so use once_into_js which auto-frees
    // the closure after invocation. The previous Closure::wrap + forget()
    // leaked one closure per call (per resize tick).
    let raf_closure = wasm_bindgen::closure::Closure::once_into_js(move || {
        call_fit(&fit_for_raf, &container_for_raf);
    });
    let _ = window.request_animation_frame(raf_closure.as_ref().unchecked_ref());
    // raf_closure is a JsValue here; keep it alive in this scope until the
    // RAF fires by... actually once_into_js hands ownership to JS. The RAF
    // holds a reference until it fires, then the closure frees itself.
}

// `debounced_fit` was deleted: it was dead code (never called) and leaked a
// Closure per invocation via forget(). schedule_fit (above) + the
// ResizeObserver's own 50ms timer in ro_closure handle debouncing.

// scan_for_resume_id has been replaced by ResumeScanner in utils::resume_scanner.
// Kept out to prevent stale references.
