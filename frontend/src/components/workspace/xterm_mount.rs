use crate::stores::terminal::TerminalSession;
use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{use_workspace_store, AgentType};
use crate::tauri_bridge::{
    pty_attach_listener, pty_default_shell_cached, pty_has_session, pty_listen_raw, pty_resize,
    pty_set_raw_paused, pty_set_xterm, pty_spawn, pty_write, read_clipboard_text,
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
    /// Rooted `@xterm/addon-serialize` instance. Kept alive for the mount so
    /// `serialize_buffer` can read the live buffer in `use_drop` *before*
    /// `term.dispose()` destroys it. Dropping this JsValue after dispose lets
    /// the addon's own disposables (registered against the terminal) run.
    _serialize_addon: Option<JsValue>,
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
    let fit_ref: Signal<Option<JsValue>> = use_signal(|| None);
    let mut terminal_store = use_terminal_store();
    let terminal_registry = use_terminal_registry();
    // Clone for use_drop BEFORE use_effect moves the original terminal_registry
    // into its own closure (the effect re-binds `terminal_registry` internally).
    let registry_for_drop = terminal_registry.clone();
    let workspace_store = use_workspace_store();
    let ui_state = use_ui_store();

    use_effect(move || {
        let already_init = *is_initialized.borrow();
        if already_init {
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
        let mut cleanup = cleanup;
        let mut term_ref = term_ref;
        let mut fit_ref = fit_ref;
        let window = window.clone();
        let container = container.clone();
        // Clone the registry for the spawned task. `TerminalRegistry` is a
        // cheap `Rc`-bump clone; cloning avoids a double-move through the
        // `use_effect` → `spawn` `move` captures.
        let terminal_registry = terminal_registry.clone();

        // Ensure a PTY session exists before initializing xterm, then set up
        // the terminal. Everything that touches the xterm instance happens in
        // this async block so we can `await` the backend spawn first.
        spawn(async move {
            // `terminal_store` and `terminal_registry` are captured at the
            // top of the component ABOVE (use_terminal_store() /
            // use_terminal_registry()), NOT re-fetched here. Dioxus hooks
            // (which `use_terminal_store` / `use_terminal_registry` /
            // `use_context` are) may only run synchronously during render;
            // calling them inside a `spawn(async move {...})` block panics at
            // mount with "hook list is already borrowed". Use the captured
            // bindings instead.
            let mut store = terminal_store;
            let registry = &terminal_registry;
            // One-shot membership check via the per-pane registry (Item 3): no
            // subscription is created inside this spawn.
            let has_session = registry.contains(&mount_id_for_spawn);
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
                store
                    .write()
                    .ensure_session(&terminal_registry, &mount_id_for_spawn, 80, 24);

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
            store
                .write()
                .set_session_xterm(&terminal_registry, &mount_id, true);

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
            // Font comes from the same source of truth the rest of the UI uses:
            // the --fontFamily / --fontSize CSS vars, which apply_font_to_dom
            // sets on launch (persisted) and on each picker/slider change.
            // xterm.js renders to a canvas and ignores CSS font-family, so it
            // must be fed options.fontFamily / fontSize explicitly (a hardcoded
            // value here would freeze the terminal on JetBrains Mono @ 14 and
            // ignore the Settings font picker + size slider entirely).
            let saved_font = read_css_var(&window, "--fontFamily");
            let font_family_val = if saved_font.is_empty() {
                "'JetBrains Mono', monospace".to_string()
            } else {
                saved_font
            };
            let saved_size = read_css_var(&window, "--fontSize");
            let font_size_val = saved_size
                .trim_end_matches("px")
                .parse::<f64>()
                .unwrap_or(14.0);
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("fontFamily"),
                &JsValue::from_str(&font_family_val),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("fontSize"),
                &JsValue::from_f64(font_size_val),
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

            // Wait until the container has a non-zero size before opening
            // xterm.js. On remount after a pane swap, the flex grid may not
            // have laid out yet, so the container rect can be 0×0.
            wait_for_container_size(&container).await;

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
                if key == "v"
                    && ((is_mac && meta && !ctrl) || (!is_mac && ctrl && !meta))
                    && !shift
                    && !alt
                {
                    event.prevent_default();
                    event.stop_propagation();
                    let pane_id = pane_id_keydown.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match read_clipboard_text().await {
                            Ok(text) => {
                                let bracketed = format!("\x1b[200~{}\x1b[201~", text);
                                if let Err(e) = pty_write(&pane_id, &bracketed).await {
                                    web_sys::console::error_1(
                                        &format!(
                                            "XtermMount: bracketed paste write failed: {:?}",
                                            e
                                        )
                                        .into(),
                                    );
                                }
                            }
                            Err(e) => {
                                web_sys::console::error_1(
                                    &format!("XtermMount: read_clipboard_text failed: {:?}", e)
                                        .into(),
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
            let workspace_for_scan = workspace_store;
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
                        })
                            as Box<dyn FnOnce()>);

                        let mut raf_failed = true;
                        if let Some(window) = web_sys::window() {
                            if window
                                .request_animation_frame(closure.as_ref().unchecked_ref())
                                .is_ok()
                            {
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
                    let mut ws = workspace_for_scan;
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
                Ok(u) => {
                    // A listener is now attached — tell the backend so it
                    // clears `raw_paused` and the read loop flushes any burst
                    // accumulated while paused. This makes every (re)subscribe
                    // self-heal: a session paused by a previous mount's drop
                    // (incl. a pane dropped without remount, later re-shown)
                    // revives here even if the remount takes the new-session
                    // branch. No-op if the session was never paused or doesn't
                    // exist yet on a brand-new spawn.
                    let mid_attach = mount_id.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = pty_attach_listener(&mid_attach).await;
                    });
                    u
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("XtermMount: pty_listen_raw failed: {e:?}").into(),
                    );
                    return;
                }
            };

            if reusing_existing_session {
                // One-shot snapshot via the registry (peek, no subscription).
                let existing_session = registry.peek_session(&mount_id);
                if let Some(session) = existing_session.as_ref() {
                    // Xterm grid is never populated (on_data skips it), so the
                    // legacy grid-based restore would only clear the terminal.
                    if !session.is_xterm {
                        restore_term_from_session(&term_val, session);
                    }
                }
                // Replay the serialized VT snapshot captured by use_drop
                // before the previous terminal was disposed. The serialized
                // string re-enters the alt buffer / DEC modes and rewrites
                // colors + scrollback + cursor position. Write synchronously
                // here (same tick as the subscribe Ok arm) so it lands in the
                // write-coalescer ahead of the raw_paused burst flush — the
                // addon's CSI sequences win the final paint even if the burst
                // arrives mid-replay. The snapshot is consumed once and
                // cleared so it's never replayed again on a later remount.
                let snapshot = registry
                    .peek_session(&mount_id)
                    .and_then(|s| s.serialized_snapshot);
                if let Some(snapshot) = snapshot {
                    write_str_to_term(&term_val, &snapshot);
                    if let Some(mut session) = registry.write_session(&mount_id) {
                        session.serialized_snapshot = None;
                    }
                }
                // No explicit unpause here: the `pty_attach_listener` call
                // above (in the subscribe Ok arm) already cleared `raw_paused`.
                // The backend read loop detects the true→false transition on
                // its next iteration and flushes the accumulated burst into
                // the JS write-coalescer, which writes directly into the
                // replayed terminal — the addon's buffer-switch sequences
                // ensure the burst paints into the right buffer.
            } else if let Ok(clear_val) =
                js_sys::Reflect::get(&term_val, &JsValue::from_str("clear"))
            {
                if let Ok(clear_fn) = clear_val.dyn_into::<js_sys::Function>() {
                    let _ = clear_fn.call0(&term_val);
                }
                force_redraw(&term_val);
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
            // SerializeAddon captures the live buffer (SGR colors, scrollback,
            // alt-screen, DEC modes, cursor) to a VT escape string. Used by
            // `use_drop` (via serialize_buffer) to snapshot the terminal just
            // before dispose, so the next mount's reuse-session branch can
            // replay it into the fresh terminal — surviving the pane-swap
            // remount (use_drop fires on every swap; see project-swap-remount).
            let serialize_addon = try_activate_addon(&window, "SerializeAddon", &term_val);

            let mut resize_observer_holder: Option<JsValue> = None;
            let mut ro_closure_holder: Option<wasm_bindgen::closure::Closure<dyn FnMut()>> = None;
            if let Some(fit_instance) = try_activate_addon(&window, "FitAddon", &term_val) {
                // Initial fit so the terminal has correct cols/rows before any data arrives.
                schedule_fit(&window, &fit_instance, &container, &term_val);
                // Publish the fit addon instance so reactive effects (the font
                // family/size effect below) can refit after pushing option
                // changes — a font change resizes glyph cells, so the container
                // no longer holds an integer cell grid until fit() re-runs.
                fit_ref.set(Some(fit_instance.clone()));

                let fit_for_ro = fit_instance.clone();
                let container_for_ro = container.clone();
                let term_for_ro = term_val.clone();
                let ro_timer: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
                let ro_timer_for_cb = ro_timer.clone();
                let ro_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    if let Some(w) = web_sys::window() {
                        if let Some(id) = ro_timer_for_cb.borrow_mut().take() {
                            w.clear_timeout_with_handle(id);
                        }
                        let fit_for_cb = fit_for_ro.clone();
                        let container_for_cb = container_for_ro.clone();
                        let term_for_cb = term_for_ro.clone();
                        // Single-fire timer closure — once_into_js auto-frees
                        // after the timeout fires, so this no longer leaks one
                        // Closure per resize tick.
                        let timer = wasm_bindgen::closure::Closure::once_into_js(move || {
                            if let Some(win) = web_sys::window() {
                                // fit() then refresh() in the same rAF tick — on
                                // a pure pane-swap relayout (only flex weight
                                // changes, no new PTY data) fit alone leaves the
                                // canvas stale; the refresh forces a repaint
                                // against the recomputed cell grid.
                                schedule_fit(&win, &fit_for_cb, &container_for_cb, &term_for_cb);
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
            // When the terminal is hidden inside a display:none subtree (e.g.
            // panel switch, fullscreen exit, multi-space tab switch), the
            // browser discards the canvas/DOM backing store, leaving the
            // terminal blank on restore. Detect visibility restoration and
            // refit + repaint. Order matters: fit() FIRST (recompute cols/rows
            // from the now-visible container rect), THEN refresh(0, rows-1)
            // with rows read post-fit so the repainted range matches the new
            // cell grid. Calling refresh-then-fit (the old code) painted with
            // stale rows, and calling term.fit() (the old code) was a no-op
            // because fit() lives on the FitAddon, not the Terminal.
            let term_for_vis = term_val.clone();
            let fit_ref_for_vis = fit_ref; // Signal<Option<JsValue>> is Copy
            let container_for_vis = container.clone();
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
                            if let (Some(win), Some(fit_instance)) =
                                (web_sys::window(), fit_ref_for_vis())
                            {
                                // fit-then-refresh, same rAF tick. call_fit
                                // skips on zero-dims (container not laid out
                                // yet) and refreshes the full post-fit range.
                                schedule_fit(
                                    &win,
                                    &fit_instance,
                                    &container_for_vis,
                                    &term_for_vis,
                                );
                            } else {
                                // No FitAddon on record (activation failed) —
                                // still force a repaint of whatever rows exist.
                                force_redraw(&term_for_vis);
                            }
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
                _serialize_addon: serialize_addon,
            }));
        });
    });

    // ── Reactive theme update ────────────────────────────────────────────────
    // When the app theme changes, re-read the CSS variables and push them
    // into the already-instantiated xterm.js terminal.
    let term_ref_for_theme = term_ref;
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

    // ── Reactive font update ───────────────────────────────────────────────
    // Mirrors the theme effect above. The Settings font picker + size slider
    // update the --fontFamily / --fontSize CSS vars via apply_font_to_dom, but
    // xterm.js renders to a canvas and ignores CSS, so we must push the new
    // font family/size into term.options live and refit (a font change resizes
    // glyph cells, which changes cols/rows for a fixed container size).
    // Read ui_state.* to subscribe to the Signals; read the CSS vars for the
    // actual values (apply_font_to_dom is the writer that keeps them coherent
    // with the persisted store).
    let term_ref_for_font = term_ref;
    let fit_ref_for_font = fit_ref;
    let mount_id_for_font = pane_id.clone();
    use_effect(move || {
        // Subscribe to the Signals so this effect re-runs on change.
        let _fam = ui_state.read().font_family.clone();
        let _size = ui_state.read().font_size;

        let term_opt = term_ref_for_font();
        let Some(term) = term_opt else {
            return;
        };
        let Some(window) = web_sys::window() else {
            return;
        };

        let new_family = read_css_var(&window, "--fontFamily");
        let new_size = read_css_var(&window, "--fontSize");
        if new_family.is_empty() && new_size.is_empty() {
            return;
        }

        if let Ok(options) = js_sys::Reflect::get(&term, &JsValue::from_str("options")) {
            if !new_family.is_empty() {
                let _ = js_sys::Reflect::set(
                    &options,
                    &JsValue::from_str("fontFamily"),
                    &JsValue::from_str(&new_family),
                );
            }
            if !new_size.is_empty() {
                if let Ok(px) = new_size.trim_end_matches("px").parse::<f64>() {
                    let _ = js_sys::Reflect::set(
                        &options,
                        &JsValue::from_str("fontSize"),
                        &JsValue::from_f64(px),
                    );
                }
            }

            // font change resizes glyph cells, so fit() must recompute cols/rows
            // and refresh() must repaint against that fresh grid — in that order,
            // in one rAF tick. (The old code fit-deferred-then-refresh-now,
            // painting stale rows until the next data burst.)
            if let Some(fit) = fit_ref_for_font() {
                if let Some(doc) = window.document() {
                    if let Some(el) = doc.get_element_by_id(&mount_id_for_font) {
                        schedule_fit(&window, &fit, &el, &term);
                    }
                }
            }
        }
    });

    let mount_id_for_drop = pane_id.clone();
    use_drop(move || {
        if let Some(mut c) = cleanup.take() {
            // Pause backend raw emission BEFORE unlistening — closes the
            // stream-gap desync window. The backend keeps reading the PTY fd
            // (shell doesn't block) but suppresses pty:raw events so no bytes are
            // lost to a dead listener during the remount gap. The new mount
            // unpauses via pty_attach_listener on re-subscribe. This must fire
            // even if the cleanup below fails, so it's the first thing.
            let mid = mount_id_for_drop.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = pty_set_raw_paused(&mid, true).await;
            });
            // Capture the live buffer BEFORE term.dispose() destroys it.
            // The next mount's reuse-session branch replays this string into
            // the fresh terminal, surviving the pane-swap remount (use_drop
            // fires on every within-space swap — confirmed at runtime; see
            // project-swap-remount). Colors, scrollback, alt-screen, DEC
            // modes, and cursor position are all preserved by the addon's
            // defaults (excludeAltBuffer:false, excludeModes:false).
            if let Some(ref serialize_addon) = c._serialize_addon {
                if let Some(snapshot) = serialize_buffer(serialize_addon) {
                    if let Some(mut session) = registry_for_drop.write_session(&mount_id_for_drop) {
                        session.serialized_snapshot = Some(snapshot);
                    }
                }
            }
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

/// Serialize the live xterm.js buffer to a VT escape string via
/// `@xterm/addon-serialize`. Returns `None` if the addon isn't loaded or the
/// call fails. MUST run BEFORE `term.dispose()` — once dispose() runs the
/// buffer (colors, scrollback, alt-screen, modes, cursor) is gone.
///
/// `excludeAltBuffer:false, excludeModes:false` (the defaults) preserve the
/// alt-screen state and DEC private modes so vim/htop round-trip correctly:
/// the serialized output begins with the buffer-switch + mode-set sequences
/// needed to re-enter that state on replay.
fn serialize_buffer(serialize_addon: &JsValue) -> Option<String> {
    let serialize_val =
        js_sys::Reflect::get(serialize_addon, &JsValue::from_str("serialize")).ok()?;
    let serialize_fn = serialize_val.dyn_into::<js_sys::Function>().ok()?;
    // { excludeAltBuffer: false, excludeModes: false } — preserve everything.
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("excludeAltBuffer"),
        &JsValue::from_bool(false),
    );
    let _ = js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("excludeModes"),
        &JsValue::from_bool(false),
    );
    let result = serialize_fn.call1(serialize_addon, &opts).ok()?;
    result.as_string()
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

// fit() recomputes cols/rows from the container rect, but on a pure geometry
// change with no new PTY bytes flowing the CanvasAddon does not always emit a
// full repaint frame — leaving the canvas showing stale glyph geometry (the
// "blank until resize" symptom after a pane drag-swap, since Dioxus only moves
// the keyed node and changes its flex weight, no remount, no new data).
// Forcing refresh(0, rows-1) with rows read *after* fit() in the same rAF tick
// repaints the whole row range against the fresh cell grid.
fn call_fit(fit_instance: &JsValue, container: &web_sys::Element, term_val: &JsValue) {
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

    // Refresh must follow fit (not precede it) and read rows *after* fit so the
    // row range matches the new cell grid. Same rAF tick — no deferral.
    refresh_full(term_val);
}

fn schedule_fit(
    window: &web_sys::Window,
    fit_instance: &JsValue,
    container: &web_sys::Element,
    term_val: &JsValue,
) {
    let fit_for_raf = fit_instance.clone();
    let container_for_raf = container.clone();
    let term_for_raf = term_val.clone();
    // RAF callbacks fire exactly once, so use once_into_js which auto-frees
    // the closure after invocation. The previous Closure::wrap + forget()
    // leaked one closure per call (per resize tick).
    let raf_closure = wasm_bindgen::closure::Closure::once_into_js(move || {
        call_fit(&fit_for_raf, &container_for_raf, &term_for_raf);
    });
    let _ = window.request_animation_frame(raf_closure.as_ref().unchecked_ref());
}

// `debounced_fit` was deleted: it was dead code (never called) and leaked a
// Closure per invocation via forget(). schedule_fit (above) + the
// ResizeObserver's own 50ms timer in ro_closure handle debouncing.

// ---------------------------------------------------------------------------
// Force a full xterm.js repaint of the visible row range: refresh(0, rows-1).
// refresh() just re-runs the renderer over the current buffer (normal *or*
// alt-screen) — it touches no buffer state, scrollback, modes, or cursor, so
// it is safe to call after any dimension change. Callers MUST have already
// run FitAddon.fit() (which recomputes cols/rows) before this, so the row
// range here matches the new cell grid. This is the post-fit half of the
// fit-then-refresh invariant used by IntersectionObserver, the font effect,
// and the post-mount size-gate path.
// ---------------------------------------------------------------------------
fn refresh_full(term_val: &JsValue) {
    let rows = js_sys::Reflect::get(term_val, &JsValue::from_str("rows"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|v| v as i32)
        .unwrap_or(24);
    if rows <= 0 {
        return;
    }
    if let Ok(refresh_val) = js_sys::Reflect::get(term_val, &JsValue::from_str("refresh")) {
        if let Ok(refresh_fn) = refresh_val.dyn_into::<js_sys::Function>() {
            let _ = refresh_fn.call2(
                term_val,
                &JsValue::from_f64(0.0),
                &JsValue::from_f64((rows - 1) as f64),
            );
        }
    }
}

// Kept for the IntersectionObserver visibility-restore path. Now refresh-only
// (fit is run separately via the FitAddon instance the observer captures),
// preserving the fit-then-refresh ordering invariant.
fn force_redraw(term_val: &JsValue) {
    refresh_full(term_val);
}

// ---------------------------------------------------------------------------
// Wait until the container has a non-zero size before opening xterm.
// On remount after a pane swap, the flex grid may not have laid out yet,
// so the container rect can be 0×0. Polling with RAF gives the browser a
// chance to reflow. Capped at ~300ms to avoid hanging indefinitely.
// ---------------------------------------------------------------------------
async fn wait_for_container_size(container: &web_sys::Element) {
    for _ in 0..15 {
        let rect = container.get_bounding_client_rect();
        if rect.width() > 0.0 && rect.height() > 0.0 {
            return;
        }
        // Yield to the browser so it can process the layout task queue.
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        // Use setTimeout to yield control back to the browser; a naive
        // Promise without resolving would deadlock forever.
        let promise = js_sys::Promise::new(&mut move |resolve, _reject| {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 20);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
    web_sys::console::warn_1(
        &"[XtermMount] container still 0-sized after 15 frames; proceeding anyway".into(),
    );
}

// scan_for_resume_id has been replaced by ResumeScanner in utils::resume_scanner.
// Kept out to prevent stale references.
