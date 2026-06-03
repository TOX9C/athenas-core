use crate::stores::terminal::use_terminal_store;
use crate::stores::terminal::TerminalSession;
use crate::tauri_bridge::{
    pty_default_shell_cached, pty_listen_raw, pty_resize, pty_spawn, pty_write,
};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

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
    /// Fallback polling watcher for WKWebView/flex layouts where ResizeObserver
    /// does not always fire during pane resizing.
    _size_watch_interval_id: Option<i32>,
    /// Rooted interval callback closure for the size watcher.
    _size_watch_closure: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
}

/// Mount an xterm.js Terminal into a div with id `pane_id`.
///
/// Subscribes to the raw PTY output stream for the pane's session and
/// pipes incoming bytes directly into the terminal.
#[component]
pub fn XtermMount(pane_id: String, cwd: String) -> Element {
    let mount_id = pane_id.clone();
    let mut cleanup: Signal<Option<XtermCleanup>> = use_signal(|| None);
    let mut is_initialized = use_signal(|| false);
    let term_ref: Signal<Option<JsValue>> = use_signal(|| None);
    let mut terminal_store = use_terminal_store();

    use_effect(move || {
        if is_initialized() {
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

        is_initialized.set(true);

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
            let reusing_existing_session = has_session;
            if !has_session {
                let shell = pty_default_shell_cached().await;
                web_sys::console::log_1(
                    &format!(
                        "[XtermMount] spawning PTY id={} cwd={} shell={} cols=80 rows=24",
                        mount_id_for_spawn, spawn_cwd, shell
                    )
                    .into(),
                );
                if let Err(e) = pty_spawn(&mount_id_for_spawn, &spawn_cwd, &shell, 80, 24).await {
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
                store.write().ensure_session(&mount_id_for_spawn, 80, 24);
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

            let term_for_write = term_val.clone();
            let unlisten = match pty_listen_raw(&mount_id, move |bytes: Vec<u8>| {
                write_bytes_to_term(&term_for_write, &bytes);
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
                        let _ = pty_write(&pane_id, &data).await;
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

            let mut resize_observer_holder: Option<JsValue> = None;
            let mut ro_closure_holder: Option<wasm_bindgen::closure::Closure<dyn FnMut()>> = None;
            let mut size_watch_interval_id: Option<i32> = None;
            let mut size_watch_closure_holder: Option<wasm_bindgen::closure::Closure<dyn FnMut()>> =
                None;
            if let Some(fit_instance) = try_activate_addon(&window, "FitAddon", &term_val) {
                schedule_fit(&window, &fit_instance);

                let fit_for_ro = fit_instance.clone();
                let ro_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    if let Some(w) = web_sys::window() {
                        schedule_fit(&w, &fit_for_ro);
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

                let last_size = Rc::new(RefCell::new((0_i32, 0_i32)));
                let last_size_for_watch = last_size.clone();
                let fit_for_size_watch = fit_instance.clone();
                let container_for_size_watch = container.clone();
                let size_watch_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    let rect = container_for_size_watch.get_bounding_client_rect();
                    let width = rect.width().round() as i32;
                    let height = rect.height().round() as i32;
                    let mut last = last_size_for_watch.borrow_mut();
                    if width > 0 && height > 0 && (width, height) != *last {
                        *last = (width, height);
                        if let Some(w) = web_sys::window() {
                            schedule_fit(&w, &fit_for_size_watch);
                        }
                    }
                })
                    as Box<dyn FnMut()>);
                if let Ok(interval_id) = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        size_watch_closure.as_ref().unchecked_ref(),
                        75,
                    )
                {
                    size_watch_interval_id = Some(interval_id);
                    size_watch_closure_holder = Some(size_watch_closure);
                }
            }
            let _ = try_activate_addon(&window, "WebLinksAddon", &term_val);
            let _ = try_activate_addon(&window, "Unicode11Addon", &term_val);

            cleanup.set(Some(XtermCleanup {
                term: term_val,
                unlisten: Some(unlisten),
                _on_data_closure: on_data_closure_js,
                _on_resize_closure: on_resize_closure_js,
                _resize_observer: resize_observer_holder,
                _ro_closure: ro_closure_holder,
                _size_watch_interval_id: size_watch_interval_id,
                _size_watch_closure: size_watch_closure_holder,
            }));
        });
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
            if let Some(interval_id) = c._size_watch_interval_id.take() {
                if let Some(window) = web_sys::window() {
                    window.clear_interval_with_handle(interval_id);
                }
            }
            let _ = c._size_watch_closure.take();
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
            style: "width: 100%; height: 100%; min-height: 0; flex: 1; background: var(--bg); position: relative;",
            onpointerdown: move |e| {
                e.stop_propagation();
                terminal_store.write().set_active(pane_id.clone());
                // Focus the xterm.js instance. Use the stored term_ref so we
                // don't create a stale closure over a temporary JsValue.
                if let Some(term) = term_ref() {
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

fn call_fit(fit_instance: &JsValue) {
    let Ok(fit_val) = js_sys::Reflect::get(fit_instance, &JsValue::from_str("fit")) else {
        return;
    };
    let Ok(fit_fn) = fit_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = fit_fn.call0(fit_instance);
}

fn schedule_fit(window: &web_sys::Window, fit_instance: &JsValue) {
    let fit_for_raf = fit_instance.clone();
    let raf_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
        call_fit(&fit_for_raf);
    }) as Box<dyn FnMut()>);
    let _ = window.request_animation_frame(raf_closure.as_ref().unchecked_ref());
    raf_closure.forget();
}
