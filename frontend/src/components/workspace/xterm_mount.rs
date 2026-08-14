use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{use_workspace_store, AgentType};
use crate::tauri_bridge::{
    pty_attach_listener, pty_default_shell_cached, pty_detach_listener, pty_has_session,
    pty_listen_raw, pty_resize, pty_set_xterm, pty_spawn, pty_spawn_agent, pty_write,
    read_clipboard_text,
};
use crate::utils::agent_commands::get_agent_command;
use crate::utils::resume_scanner::ResumeScanner;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[path = "xterm_helpers.rs"]
mod xterm_helpers;
use xterm_helpers::{
    force_redraw, read_css_var, restore_term_from_session, schedule_fit, serialize_buffer,
    try_activate_addon, wait_for_container_size, write_bytes_to_term, write_str_to_term,
};

/// Default scrollback buffer size for xterm.js sessions.
/// Previously hardcoded to 2500; raised to 10000 to
/// accommodate long-running build/logs without truncation.
const XTERM_SCROLLBACK: f64 = 10000.0;
/// PTY output already contains the shell's cursor-control and line-ending
/// bytes. Let xterm.js interpret those bytes exactly as received; converting
/// EOLs is intended for text streams and corrupts Unix PTY redraw sequences.
const XTERM_CONVERT_EOL: bool = false;

#[cfg(test)]
mod tests {
    use super::active_pane_matches;

    #[test]
    fn active_focus_only_targets_the_selected_pane() {
        assert!(active_pane_matches(Some("pane-1"), "pane-1"));
        assert!(!active_pane_matches(Some("pane-2"), "pane-1"));
        assert!(!active_pane_matches(None, "pane-1"));
    }
}

/// Focus the xterm hidden textarea and retry once after WebKit completes the
/// DOM/layout work triggered by `open()`. Interactive TUIs render without this
/// focus, but xterm.js has nowhere to send their key events.
fn focus_xterm(term: &JsValue) {
    if let Ok(focus_val) = js_sys::Reflect::get(term, &JsValue::from_str("focus")) {
        if let Ok(focus_fn) = focus_val.dyn_into::<js_sys::Function>() {
            let _ = focus_fn.call0(term);
        }
    }
}

/// Do not steal focus from a real form control when the app resumes. The
/// xterm helper textarea is the one editable element that is safe to recover.
fn focus_recovery_allowed(document: &web_sys::Document) -> bool {
    let Some(active) = document.active_element() else {
        return true;
    };
    let tag = active.tag_name().to_ascii_lowercase();
    if tag == "input" || tag == "textarea" || active.has_attribute("contenteditable") {
        return active
            .get_attribute("class")
            .map(|classes| {
                classes
                    .split_ascii_whitespace()
                    .any(|class_name| class_name == "xterm-helper-textarea")
            })
            .unwrap_or(false);
    }
    true
}

fn document_is_visible(document: &web_sys::Document) -> bool {
    js_sys::Reflect::get(document, &JsValue::from_str("visibilityState"))
        .ok()
        .and_then(|value| value.as_string())
        .map(|state| state == "visible")
        .unwrap_or(true)
}

fn active_pane_matches(active_session_id: Option<&str>, pane_id: &str) -> bool {
    active_session_id == Some(pane_id)
}

/// Return whether this mount is the first visible xterm and no helper textarea
/// currently owns focus. This is the renderer-only fallback for the short
/// window before the terminal store's active-pane effect catches up.
fn initial_xterm_focus_candidate(
    document: &web_sys::Document,
    container: &web_sys::Element,
) -> bool {
    let helper_focused = document
        .active_element()
        .and_then(|active| active.get_attribute("class"))
        .map(|classes| {
            classes
                .split_ascii_whitespace()
                .any(|class_name| class_name == "xterm-helper-textarea")
        })
        .unwrap_or(false);
    if helper_focused || !focus_recovery_allowed(document) {
        return false;
    }

    let mounts = document
        .query_selector_all(".xterm-mount[data-terminal-renderer=\"xterm\"]")
        .ok();
    let Some(mounts) = mounts else {
        return false;
    };
    for index in 0..mounts.length() {
        let Some(mount) = mounts
            .item(index)
            .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        let rect = mount.get_bounding_client_rect();
        if rect.width() > 0.0 && rect.height() > 0.0 {
            return mount.is_same_node(Some(container.as_ref()));
        }
    }
    false
}

fn schedule_initial_xterm_focus(
    window: &web_sys::Window,
    term: &JsValue,
    active: &Rc<RefCell<bool>>,
) {
    if !*active.borrow() {
        return;
    }
    focus_xterm(term);
    let term_for_frame = term.clone();
    let active_for_frame = active.clone();
    let frame = Closure::once_into_js(move || {
        if *active_for_frame.borrow() {
            focus_xterm(&term_for_frame);
        }
    });
    let _ = window.request_animation_frame(frame.as_ref().unchecked_ref());
}

fn schedule_xterm_focus(
    window: &web_sys::Window,
    term: &JsValue,
    active: &Rc<RefCell<bool>>,
    pane_id: &str,
    terminal_store: Signal<crate::stores::terminal::TerminalStore>,
) {
    if !*active.borrow()
        || !active_pane_matches(terminal_store.peek().active_session_id.as_deref(), pane_id)
    {
        return;
    }
    focus_xterm(term);
    let term_for_frame = term.clone();
    let active_for_frame = active.clone();
    let pane_id_for_frame = pane_id.to_string();
    let frame = Closure::once_into_js(move || {
        if *active_for_frame.borrow()
            && active_pane_matches(
                terminal_store.peek().active_session_id.as_deref(),
                pane_id_for_frame.as_str(),
            )
        {
            focus_xterm(&term_for_frame);
        }
    });
    let _ = window.request_animation_frame(frame.as_ref().unchecked_ref());
}

fn enqueue_pty_input(
    queue: &Rc<RefCell<VecDeque<String>>>,
    draining: &Rc<RefCell<bool>>,
    active: &Rc<RefCell<bool>>,
    pane_id: &str,
    data: String,
) {
    if !*active.borrow() {
        return;
    }
    queue.borrow_mut().push_back(data);
    if *draining.borrow() {
        return;
    }
    *draining.borrow_mut() = true;

    let queue = queue.clone();
    let draining = draining.clone();
    let active = active.clone();
    let pane_id = pane_id.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            if !*active.borrow() {
                queue.borrow_mut().clear();
                *draining.borrow_mut() = false;
                break;
            }
            let next = queue.borrow_mut().pop_front();
            let Some(data) = next else {
                *draining.borrow_mut() = false;
                break;
            };
            if let Err(e) = pty_write(&pane_id, &data).await {
                web_sys::console::error_1(&format!("XtermMount: pty_write failed: {:?}", e).into());
            }
        }
    });
}

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
    /// Pending ResizeObserver debounce timer. Cleared on unmount so a delayed
    /// callback cannot retain a disposed terminal longer than necessary.
    _ro_timer: Option<Rc<RefCell<Option<i32>>>>,
    /// Rooted keydown handler for custom macOS keyboard shortcuts.
    _keydown_handler: Option<JsValue>,
    /// Capture-phase pointer handler. xterm/WebKit can stop bubbling pointer
    /// events before the Dioxus wrapper sees them, which otherwise leaves the
    /// previously active pane's hidden textarea focused.
    _pointerdown_handler: Option<JsValue>,
    /// Container on which the capture-phase pointer handler is registered.
    _pointerdown_container: Option<web_sys::Element>,
    /// Shared input-lifecycle flag; set false before a mount is disposed so
    /// queued or delayed shortcut input cannot reach a later pane instance.
    input_active: Option<Rc<RefCell<bool>>>,
    /// IntersectionObserver that detects when the terminal container
    /// becomes visible after being hidden (e.g. display:none toggle).
    _visibility_observer: Option<JsValue>,
    /// Rooted IntersectionObserver callback closure.
    _vis_callback: Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>>,
    /// Listener lease generation shared with the attach/detach IPC tasks.
    listener_generation: Option<Rc<RefCell<Option<u64>>>>,
    /// Focus recovery listener installed on the window/document. WebKit can
    /// lose the hidden textarea's native focus after app resume even while the
    /// terminal canvas remains rendered.
    _focus_recovery_handler: Option<JsValue>,
    _focus_recovery_window: Option<web_sys::Window>,
    _focus_recovery_document: Option<web_sys::Document>,
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
    bypass_mode: Option<bool>,
) -> Element {
    let mount_id = pane_id.clone();
    let listener_owner: Rc<String> =
        use_hook(|| Rc::new(format!("desktop:{}", js_sys::Math::random())));
    let listener_owner_for_effect = listener_owner.clone();
    let mut cleanup: Signal<Option<XtermCleanup>> = use_signal(|| None);
    // The async mount task can outlive the component by a few turns while
    // PTY/session setup and xterm loading are in flight. Keep a lifecycle flag
    // from the first render so a late task cannot install a raw listener after
    // `use_drop` has already run.
    let mount_active = use_hook(|| Rc::new(RefCell::new(true)));
    let is_initialized = use_hook(|| Rc::new(RefCell::new(false)));
    let term_ref: Signal<Option<JsValue>> = use_signal(|| None);
    let fit_ref: Signal<Option<JsValue>> = use_signal(|| None);
    // Coalesce fit requests across ResizeObserver, visibility restoration,
    // font changes, and pane relayouts. This flag belongs to the component,
    // not to one async mount task, so reactive effects can share it safely.
    let fit_pending: Rc<RefCell<bool>> = use_hook(|| Rc::new(RefCell::new(false)));
    let fit_pending_for_effect = fit_pending.clone();
    let mut terminal_store = use_terminal_store();
    let terminal_registry = use_terminal_registry();
    // Clone for use_drop BEFORE use_effect moves the original terminal_registry
    // into its own closure (the effect re-binds `terminal_registry` internally).
    let registry_for_drop = terminal_registry.clone();
    let workspace_store = use_workspace_store();
    let ui_state = use_ui_store();
    let mount_active_for_drop = mount_active.clone();

    use_effect(move || {
        let fit_pending_for_task = fit_pending_for_effect.clone();
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
        let agent_command_for_spawn = get_agent_command(
            &agent_type,
            custom_cmd.as_deref(),
            bypass_mode.unwrap_or(false),
        );
        let mount_id_for_spawn = mount_id.clone();
        let spawn_cwd = if cwd.trim().is_empty() {
            "/tmp".to_string()
        } else {
            cwd.clone()
        };
        let mut cleanup = cleanup;
        let mut term_ref = term_ref;
        let mut fit_ref = fit_ref;
        let mount_active_for_task = mount_active.clone();
        let window = window.clone();
        let container = container.clone();
        // Clone the registry for the spawned task. `TerminalRegistry` is a
        // cheap `Rc`-bump clone; cloning avoids a double-move through the
        // `use_effect` → `spawn` `move` captures.
        let terminal_registry = terminal_registry.clone();

        // Ensure a PTY session exists before initializing xterm, then set up
        // the terminal. Everything that touches the xterm instance happens in
        // this async block so we can `await` the backend spawn first.
        let listener_owner_for_task = listener_owner_for_effect.clone();
        spawn(async move {
            if !*mount_active_for_task.borrow() {
                return;
            }
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
            if !*mount_active_for_task.borrow() {
                return;
            }
            let reusing_existing_session = has_session || has_backend;
            if !has_session {
                if !has_backend {
                    let shell = pty_default_shell_cached().await;
                    if !*mount_active_for_task.borrow() {
                        return;
                    }
                    let launch_command = agent_command_for_spawn.clone();
                    web_sys::console::log_1(
                        &format!(
                            "[XtermMount] spawning PTY id={} cwd={} shell={} cols=80 rows=24 agent={:?}",
                            mount_id_for_spawn, spawn_cwd, shell, launch_command
                        )
                        .into(),
                    );
                    let spawn_result = if let Some(agent_cmd) = launch_command.as_deref() {
                        pty_spawn_agent(
                            &mount_id_for_spawn,
                            &spawn_cwd,
                            &shell,
                            &format!("{}\n", agent_cmd),
                            80,
                            24,
                            true,
                            Some(listener_owner_for_task.as_str()),
                        )
                        .await
                    } else {
                        pty_spawn(
                            &mount_id_for_spawn,
                            &spawn_cwd,
                            &shell,
                            80,
                            24,
                            true,
                            Some(listener_owner_for_task.as_str()),
                        )
                        .await
                    };
                    if let Err(e) = spawn_result {
                        web_sys::console::error_1(
                            &format!(
                                "XtermMount: PTY spawn failed for id={} cwd={} shell={} agent={:?}: {e:?}",
                                mount_id_for_spawn, spawn_cwd, shell, agent_command_for_spawn
                            )
                            .into(),
                        );
                        return;
                    }
                    if !*mount_active_for_task.borrow() {
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
                // Agent commands are written by `pty_spawn_agent` before its
                // reader starts, so no separate fire-and-forget write is needed.
                // Keep the captured values alive for the resume/bootstrap path.
                let _ = (&custom_cmd_for_spawn, &agent_type_for_spawn);
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
            if !*mount_active_for_task.borrow() {
                return;
            }

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
            // Supply the complete ANSI palette explicitly. WebKit/xterm can
            // otherwise fall back to a nearly monochrome palette when the
            // surrounding app theme only defines foreground/background.
            let ansi_palette = [
                ("black", "#1b1b1b"),
                ("red", "#e06c75"),
                ("green", "#98c379"),
                ("yellow", "#e5c07b"),
                ("blue", "#61afef"),
                ("magenta", "#c678dd"),
                ("cyan", "#56b6c2"),
                ("white", "#d7dae0"),
                ("brightBlack", "#5c6370"),
                ("brightRed", "#e06c75"),
                ("brightGreen", "#98c379"),
                ("brightYellow", "#e5c07b"),
                ("brightBlue", "#61afef"),
                ("brightMagenta", "#c678dd"),
                ("brightCyan", "#56b6c2"),
                ("brightWhite", "#ffffff"),
            ];
            for (name, color) in ansi_palette {
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str(name),
                    &JsValue::from_str(color),
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
            // Be explicit for full-screen CLIs: the default is false, but
            // WebKit/xterm integrations can inherit a stale option when a
            // terminal is remounted.
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("disableStdin"),
                &JsValue::from_bool(false),
            );
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("convertEol"),
                &JsValue::from_bool(XTERM_CONVERT_EOL),
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
            if !*mount_active_for_task.borrow() {
                return;
            }

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

            // xterm.js keeps keyboard input on a hidden textarea. The app root
            // is focusable and is also the target of global shortcuts, so the
            // active pane can otherwise render while focus remains on the
            // root. Only the active pane gets automatic focus: focusing every
            // pane in a grid would let the last async mount steal focus from
            // the pane the user selected. The pointer handler below focuses
            // whichever pane the user clicks.
            let active_pane = store
                .read()
                .active_session_id
                .as_deref()
                .is_some_and(|id| id == mount_id.as_str());
            let initial_focus_claimed =
                !active_pane && initial_xterm_focus_candidate(&document, &container);
            if active_pane {
                schedule_xterm_focus(&window, &term_val, &mount_active_for_task, &mount_id, store);
            } else if initial_focus_claimed {
                // A newly-created workspace can mount its first xterm before
                // TerminalController publishes the active pane. Focus the
                // first visible helper only when no other helper is focused;
                // later panes therefore cannot steal the user's keyboard.
                // This fallback is intentionally focus-only. The controller
                // and pointer path remain the owners of TerminalStore active
                // state; coupling async renderer recovery to that store would
                // let a remount race change global shortcut routing.
                schedule_initial_xterm_focus(&window, &term_val, &mount_active_for_task);
            }

            // ── Custom keyboard shortcuts (macOS) ────────────────────────────
            // xterm.js does not send distinct sequences for Shift+Enter or
            // Cmd+Delete. We intercept them in capture phase, enqueue the
            // appropriate escape sequence, and prevent xterm.js from also
            // forwarding its default sequence.
            let input_queue: Rc<RefCell<VecDeque<String>>> = Rc::new(RefCell::new(VecDeque::new()));
            let input_draining = Rc::new(RefCell::new(false));
            let pane_id_keydown = mount_id.clone();
            let input_queue_for_keydown = input_queue.clone();
            let input_draining_for_keydown = input_draining.clone();
            let input_active = Rc::new(RefCell::new(true));
            let input_active_for_keydown = input_active.clone();
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
                    let queue = input_queue_for_keydown.clone();
                    let draining = input_draining_for_keydown.clone();
                    let active = input_active_for_keydown.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match read_clipboard_text().await {
                            Ok(text) => {
                                let bracketed = format!("\x1b[200~{}\x1b[201~", text);
                                enqueue_pty_input(&queue, &draining, &active, &pane_id, bracketed);
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
                    enqueue_pty_input(
                        &input_queue_for_keydown,
                        &input_draining_for_keydown,
                        &input_active_for_keydown,
                        &pane_id_keydown,
                        "\n".to_string(),
                    );
                    return;
                }

                // Cmd+Delete (Backspace) → delete to beginning of line
                // Maps to readline's unix-line-discard (Ctrl+U).
                if key == "Backspace" && meta && !shift && !ctrl && !alt {
                    event.prevent_default();
                    event.stop_propagation();
                    enqueue_pty_input(
                        &input_queue_for_keydown,
                        &input_draining_for_keydown,
                        &input_active_for_keydown,
                        &pane_id_keydown,
                        "\x15".to_string(),
                    );
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
            if !*mount_active_for_task.borrow() {
                return;
            }

            let listener_generation = Rc::new(RefCell::new(None));
            let listener_owner_for_attach = listener_owner_for_task.clone();
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
                    // The component may have been dropped while the native
                    // listener registration was being prepared. Unregister
                    // immediately instead of handing a stale listener to a
                    // cleanup signal that no longer exists.
                    if !*mount_active_for_task.borrow() {
                        u();
                        let mid_cancel = mount_id.clone();
                        let owner_cancel = listener_owner_for_attach.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ =
                                pty_detach_listener(&mid_cancel, owner_cancel.as_str(), 0).await;
                        });
                        return;
                    }
                    // A listener is now attached — tell the backend so it
                    // clears `raw_paused` and the read loop flushes any burst
                    // accumulated while paused. This makes every (re)subscribe
                    // self-heal: a session paused by a previous mount's drop
                    // (incl. a pane dropped without remount, later re-shown)
                    // revives here even if the remount takes the new-session
                    // branch. No-op if the session was never paused or doesn't
                    // exist yet on a brand-new spawn.
                    let mid_attach = mount_id.clone();
                    let owner_attach = listener_owner_for_attach.clone();
                    let listener_generation_for_attach = listener_generation.clone();
                    let active_for_attach = mount_active_for_task.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        // A freshly spawned PTY can become visible to the
                        // command handler one turn after the raw listener is
                        // registered. Retry the zero-generation response so a
                        // start-paused session cannot remain muted forever.
                        for attempt in 0..4 {
                            if !*active_for_attach.borrow() {
                                return;
                            }
                            match pty_attach_listener(
                                &mid_attach,
                                owner_attach.as_str(),
                                reusing_existing_session,
                            )
                            .await
                            {
                                Ok(generation) if generation != 0 => {
                                    *listener_generation_for_attach.borrow_mut() = Some(generation);
                                    if !*active_for_attach.borrow() {
                                        let _ = pty_detach_listener(
                                            &mid_attach,
                                            owner_attach.as_str(),
                                            generation,
                                        )
                                        .await;
                                    }
                                    return;
                                }
                                Ok(_) if attempt < 3 => {
                                    gloo::timers::future::TimeoutFuture::new(25).await;
                                }
                                Ok(_) => {
                                    web_sys::console::warn_1(
                                        &format!(
                                            "XtermMount: listener attach returned no session for {}",
                                            mid_attach
                                        )
                                        .into(),
                                    );
                                    let _ =
                                        pty_detach_listener(&mid_attach, owner_attach.as_str(), 0)
                                            .await;
                                    return;
                                }
                                Err(error) if attempt < 3 => {
                                    web_sys::console::warn_1(
                                        &format!(
                                            "XtermMount: listener attach retry {} for {}: {error:?}",
                                            attempt + 1,
                                            mid_attach
                                        )
                                        .into(),
                                    );
                                    gloo::timers::future::TimeoutFuture::new(25).await;
                                }
                                Err(error) => {
                                    web_sys::console::error_1(
                                        &format!(
                                            "XtermMount: pty_attach_listener failed: {error:?}"
                                        )
                                        .into(),
                                    );
                                    let _ =
                                        pty_detach_listener(&mid_attach, owner_attach.as_str(), 0)
                                            .await;
                                    return;
                                }
                            }
                        }
                    });
                    // The attach task also owns a clone, so teardown-before-
                    // attach is safe: the task detaches its generation after
                    // it sees the mount is inactive.
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
            } // xterm.js delivers input synchronously, but each Tauri invoke is
              // asynchronous. Route all input through the same per-pane drain,
              // including the custom shortcuts above, so IPC completions cannot
              // overlap or reorder bytes from one terminal.
            let pane_id_for_data = mount_id.clone();
            let input_queue_for_data = input_queue.clone();
            let input_draining_for_data = input_draining.clone();
            let input_active_for_data = input_active.clone();
            let on_data_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |data: String| {
                    enqueue_pty_input(
                        &input_queue_for_data,
                        &input_draining_for_data,
                        &input_active_for_data,
                        &pane_id_for_data,
                        data,
                    );
                }) as Box<dyn FnMut(String)>);
            let on_data_closure_js = on_data_closure.into_js_value();
            if let Some(on_data_fn) = js_sys::Reflect::get(&term_val, &JsValue::from_str("onData"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
            {
                let _ = on_data_fn.call1(&term_val, on_data_closure_js.as_ref());
            }

            // Re-focus the active pane after registering `onData`. This is
            // intentionally repeated: xterm's `open()` creates and focuses
            // its hidden textarea asynchronously in some WebKit builds, and
            // the PTY bootstrap/restore work above can otherwise leave the
            // document's focus on the app root. Without this, an interactive
            // full-screen CLI (notably Oh My Pi) can render correctly but
            // receive no ordinary keys or Ctrl-C until the focus surface is
            // clicked.
            if store
                .read()
                .active_session_id
                .as_deref()
                .is_some_and(|id| id == mount_id.as_str())
            {
                schedule_xterm_focus(&window, &term_val, &mount_active_for_task, &mount_id, store);
            } else if initial_focus_claimed {
                // Repeat after onData registration. WebKit can accept the
                // native focus call during open(), then move first-responder
                // state while PTY bootstrap/listener setup completes.
                schedule_initial_xterm_focus(&window, &term_val, &mount_active_for_task);
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
            let mut ro_timer_holder: Option<Rc<RefCell<Option<i32>>>> = None;
            if let Some(fit_instance) = try_activate_addon(&window, "FitAddon", &term_val) {
                // Initial fit so the terminal has correct cols/rows before any data arrives.
                schedule_fit(
                    &window,
                    &fit_instance,
                    &container,
                    &term_val,
                    &fit_pending_for_task,
                    &mount_active_for_task,
                );
                // Publish the fit addon instance so reactive effects (the font
                // family/size effect below) can refit after pushing option
                // changes — a font change resizes glyph cells, so the container
                // no longer holds an integer cell grid until fit() re-runs.
                fit_ref.set(Some(fit_instance.clone()));

                let fit_for_ro = fit_instance.clone();
                let container_for_ro = container.clone();
                let term_for_ro = term_val.clone();
                let fit_pending_for_ro = fit_pending_for_task.clone();
                let mount_active_for_ro = mount_active_for_task.clone();
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
                        let fit_pending_for_timer = fit_pending_for_ro.clone();
                        let mount_active_for_timer = mount_active_for_ro.clone();
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
                                schedule_fit(
                                    &win,
                                    &fit_for_cb,
                                    &container_for_cb,
                                    &term_for_cb,
                                    &fit_pending_for_timer,
                                    &mount_active_for_timer,
                                );
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
                        ro_timer_holder = Some(ro_timer.clone());
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
            let fit_pending_for_vis = fit_pending_for_task.clone();
            let mount_active_for_vis = mount_active_for_task.clone();
            let mount_active_for_pointer = mount_active_for_task.clone();
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
                                    &fit_pending_for_vis,
                                    &mount_active_for_vis,
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

            // Recover the hidden textarea after WKWebView/app focus resumes.
            // The canvas can remain painted while WebKit has dropped the
            // textarea's first-responder focus. Register on both window focus
            // and document visibility/page-show events, but never steal focus
            // from another real input control.
            let focus_recovery_term = term_val.clone();
            let focus_recovery_active = mount_active_for_task.clone();
            let focus_recovery_pane = mount_id.clone();
            let focus_recovery_store = terminal_store;
            let focus_recovery_handler = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                if !*focus_recovery_active.borrow() {
                    return;
                }
                let Some(window) = web_sys::window() else {
                    return;
                };
                let Some(document) = window.document() else {
                    return;
                };
                if !document_is_visible(&document) || !focus_recovery_allowed(&document) {
                    return;
                }
                schedule_xterm_focus(
                    &window,
                    &focus_recovery_term,
                    &focus_recovery_active,
                    &focus_recovery_pane,
                    focus_recovery_store,
                );
            })
                as Box<dyn FnMut(web_sys::Event)>);
            let focus_recovery_handler_js = focus_recovery_handler.into_js_value();
            let _ = window.add_event_listener_with_callback(
                "focus",
                focus_recovery_handler_js.as_ref().unchecked_ref(),
            );
            let _ = window.add_event_listener_with_callback(
                "pageshow",
                focus_recovery_handler_js.as_ref().unchecked_ref(),
            );
            if let Some(document) = window.document() {
                let _ = document.add_event_listener_with_callback(
                    "visibilitychange",
                    focus_recovery_handler_js.as_ref().unchecked_ref(),
                );
            }

            // xterm owns the canvas/textarea subtree and WebKit may stop a
            // bubbling pointer event before the Dioxus wrapper sees it.
            // Register this only after all fallible mount setup above has
            // succeeded, so a partial mount cannot leak a native listener.
            let pointer_term = term_val.clone();
            let pointer_pane_id = mount_id.clone();
            let mut pointer_terminal_store = terminal_store;
            let pointerdown_handler = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                if !*mount_active_for_pointer.borrow() {
                    return;
                }
                pointer_terminal_store
                    .write()
                    .set_active(pointer_pane_id.clone());
                if let Some(window) = web_sys::window() {
                    schedule_xterm_focus(
                        &window,
                        &pointer_term,
                        &mount_active_for_pointer,
                        &pointer_pane_id,
                        pointer_terminal_store,
                    );
                } else {
                    focus_xterm(&pointer_term);
                }
            })
                as Box<dyn FnMut(web_sys::Event)>);
            let pointerdown_handler_js = pointerdown_handler.into_js_value();
            let _ = container.add_event_listener_with_callback_and_bool(
                "pointerdown",
                pointerdown_handler_js.as_ref().unchecked_ref(),
                true,
            );

            cleanup.set(Some(XtermCleanup {
                term: term_val,
                unlisten: Some(unlisten),
                _on_data_closure: on_data_closure_js,
                _on_resize_closure: on_resize_closure_js,
                _resize_observer: resize_observer_holder,
                _ro_closure: ro_closure_holder,
                _ro_timer: ro_timer_holder,
                _keydown_handler: Some(keydown_handler_js),
                _pointerdown_handler: Some(pointerdown_handler_js),
                _pointerdown_container: Some(container.clone()),
                input_active: Some(input_active),
                _visibility_observer: vis_observer_holder,
                _vis_callback: vis_callback_holder,
                listener_generation: Some(listener_generation),
                _focus_recovery_handler: Some(focus_recovery_handler_js),
                _focus_recovery_window: Some(window.clone()),
                _focus_recovery_document: window.document(),
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
    let fit_pending_for_font = fit_pending.clone();
    let mount_id_for_font = pane_id.clone();
    let mount_active_for_font = mount_active_for_drop.clone();
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
                        schedule_fit(
                            &window,
                            &fit,
                            &el,
                            &term,
                            &fit_pending_for_font,
                            &mount_active_for_font,
                        );
                    }
                }
            }
        }
    });

    // Focus is part of the active-pane lifecycle, not only a mount/pointer
    // side effect. The xterm instance may finish opening after the controller
    // selects this pane, or the user may switch back to it after a remount.
    // Reading both signals makes this effect rerun when either condition
    // changes, then schedule_xterm_focus performs the immediate + next-frame
    // focus handoff against the current active-pane guard.
    let active_focus_term = term_ref;
    let active_focus_pane_id = pane_id.clone();
    let active_focus_mount = mount_active_for_drop.clone();
    let active_focus_store = terminal_store;
    use_effect(move || {
        let active_session_id = active_focus_store.read().active_session_id.clone();
        let Some(term) = active_focus_term() else {
            return;
        };
        if !active_pane_matches(active_session_id.as_deref(), &active_focus_pane_id) {
            return;
        }
        if let Some(window) = web_sys::window() {
            schedule_xterm_focus(
                &window,
                &term,
                &active_focus_mount,
                &active_focus_pane_id,
                active_focus_store,
            );
        }
    });

    let mount_id_for_drop = pane_id.clone();
    let mount_active_for_view = mount_active_for_drop.clone();
    use_drop(move || {
        *mount_active_for_drop.borrow_mut() = false;
        if let Some(mut c) = cleanup.take() {
            if let Some(active) = c.input_active.take() {
                *active.borrow_mut() = false;
            }
            // Detach the current listener lease before unlistening. A stale
            // teardown cannot pause a newer mount because the backend checks
            // the generation. If attach is still in flight, that task notices
            // `mount_active == false` and detaches its newly allocated lease.
            let mid = mount_id_for_drop.clone();
            let owner = listener_owner.clone();
            let generation = c
                .listener_generation
                .as_ref()
                .and_then(|generation| *generation.borrow())
                .unwrap_or(0);
            wasm_bindgen_futures::spawn_local(async move {
                let _ = pty_detach_listener(&mid, owner.as_str(), generation).await;
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
            if let Some(focus_handler) = c._focus_recovery_handler.take() {
                if let Some(focus_window) = c._focus_recovery_window.take() {
                    let _ = focus_window.remove_event_listener_with_callback(
                        "focus",
                        focus_handler.as_ref().unchecked_ref(),
                    );
                    let _ = focus_window.remove_event_listener_with_callback(
                        "pageshow",
                        focus_handler.as_ref().unchecked_ref(),
                    );
                }
                if let Some(focus_document) = c._focus_recovery_document.take() {
                    let _ = focus_document.remove_event_listener_with_callback(
                        "visibilitychange",
                        focus_handler.as_ref().unchecked_ref(),
                    );
                }
            }
            if let (Some(pointerdown_handler), Some(pointerdown_container)) = (
                c._pointerdown_handler.take(),
                c._pointerdown_container.take(),
            ) {
                let _ = pointerdown_container.remove_event_listener_with_callback_and_bool(
                    "pointerdown",
                    pointerdown_handler.as_ref().unchecked_ref(),
                    true,
                );
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
            if let Some(timer) = c._ro_timer.take() {
                if let Some(window) = web_sys::window() {
                    if let Some(handle) = timer.borrow_mut().take() {
                        window.clear_timeout_with_handle(handle);
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
        } else {
            // The mount can be dropped before xterm setup stores its cleanup
            // record. Cancel the generation-zero startup lease anyway so a
            // delayed attach from this abandoned owner cannot claim a
            // replacement mount's PTY.
            let mid = mount_id_for_drop.clone();
            let owner = listener_owner.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = pty_detach_listener(&mid, owner.as_str(), 0).await;
            });
        }

        // Permanent pane closes mark the id before removing it from the
        // workspace. Consume that marker only after this component has released
        // its listener, observers, and xterm instance. Ordinary layout/swap
        // remounts leave the session in the registry for reattachment.
        if registry_for_drop.is_closing(&mount_id_for_drop) {
            registry_for_drop.remove(&mount_id_for_drop);
        }
    });

    rsx! {
        div {
            id: "{pane_id}",
            class: "xterm-mount",
            "data-terminal-renderer": "xterm",
            "data-pane-id": "{pane_id}",
            style: "width: 100%; height: 100%; min-height: 0; flex: 1; background: var(--bg); position: relative; overflow: hidden; padding: 0; box-sizing: border-box;",
            onpointerdown: move |e| {
                e.stop_propagation();
                terminal_store.write().set_active(pane_id.clone());
                if let (Some(window), Some(term)) = (web_sys::window(), term_ref()) {
                    // Focus the xterm.js hidden textarea. Retry on the next
                    // frame because WebKit may finish its pointer default
                    // action after this handler returns.
                    schedule_xterm_focus(
                        &window,
                        &term,
                        &mount_active_for_view,
                        &pane_id,
                        terminal_store,
                    );
                }
            },
        }
    }
}
