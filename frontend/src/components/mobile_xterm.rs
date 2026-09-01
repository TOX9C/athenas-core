use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

fn write_bytes(term: &JsValue, bytes: &[u8]) {
    let Ok(write) = js_sys::Reflect::get(term, &JsValue::from_str("write")) else {
        return;
    };
    let Ok(write) = write.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = write.call1(term, js_sys::Uint8Array::from(bytes).as_ref());
}

fn write_text(term: &JsValue, text: &str) {
    let Ok(write) = js_sys::Reflect::get(term, &JsValue::from_str("write")) else {
        return;
    };
    let Ok(write) = write.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = write.call1(term, &JsValue::from_str(text));
}

fn fit(fit_addon: &JsValue) {
    if let Ok(value) = js_sys::Reflect::get(fit_addon, &JsValue::from_str("fit")) {
        if let Ok(fit) = value.dyn_into::<js_sys::Function>() {
            let _ = fit.call0(fit_addon);
        }
    }
}

fn new_addon(window: &web_sys::Window, name: &str, term: &JsValue) -> Option<JsValue> {
    let global = js_sys::Reflect::get(window, &JsValue::from_str(name)).ok()?;
    let ctor = if global.is_function() {
        global
    } else {
        js_sys::Reflect::get(&global, &JsValue::from_str(name)).ok()?
    };
    let ctor = ctor.dyn_into::<js_sys::Function>().ok()?;
    let addon = js_sys::Reflect::construct(&ctor, &js_sys::Array::new()).ok()?;
    let activate = js_sys::Reflect::get(&addon, &JsValue::from_str("activate"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    activate.call1(&addon, term).ok()?;
    Some(addon)
}

/// Enqueue terminal input. Everything queued at drain time is concatenated
/// into ONE `pty_write` — over the relay each write is a WS round trip, so
/// batching collapses a burst of keystrokes (or a paste) into a single frame
/// while preserving byte order. Per-keystroke latency is unchanged (first
/// byte still starts the drain immediately).
fn enqueue_input(
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
                return;
            }

            // Drain everything pending into one ordered write.
            let batch = {
                let mut q = queue.borrow_mut();
                if q.is_empty() {
                    *draining.borrow_mut() = false;
                    return;
                }
                let total: usize = q.iter().map(|s| s.len()).sum();
                let mut batch = String::with_capacity(total);
                for part in q.drain(..) {
                    batch.push_str(&part);
                }
                batch
            };
            if let Err(error) = tauri_bridge::pty_write(&pane_id, &batch).await {
                web_sys::console::error_1(
                    &format!("[mobile xterm] pty_write failed: {error:?}").into(),
                );
            }
        }
    });
}

/// Map a keybar character through the armed Ctrl modifier: `c` → 0x03, etc.
fn ctrl_modified(data: &str) -> Option<String> {
    if data.chars().count() != 1 {
        return None;
    }
    let c = data.chars().next()?;
    if c.is_ascii_lowercase() || c.is_ascii_uppercase() {
        let code = (c.to_ascii_lowercase() as u8) & 0x1f;
        return String::from_utf8(vec![code]).ok();
    }
    match c {
        '[' => Some("\x1b".to_string()),
        ']' => Some("\x1d".to_string()),
        _ => None,
    }
}

/// Owns the JavaScript callbacks and observer resources for one mobile
/// terminal mount. The underscore-prefixed callback fields are intentionally
/// retained even though Rust never reads them: xterm.js stores the callbacks
/// as JS references, but this struct is the lifetime root that guarantees they
/// remain callable until `term.dispose()` and observer disconnect complete.
struct MobileXtermCleanup {
    term: JsValue,
    unlisten: Option<Box<dyn FnOnce()>>,
    /// Relay binary-mode sink deregistration (paired with `unlisten`).
    unlisten_raw: Option<Box<dyn FnOnce()>>,
    /// WebGL renderer addon instance — retained so the addon (and its GPU
    /// context) outlives the setup task.
    _webgl: Option<JsValue>,
    _on_data: JsValue,
    _on_resize: JsValue,
    resize_observer: Option<JsValue>,
    /// Keep the ResizeObserver callback rooted for as long as the observer
    /// is connected; dropping it early would silently disable resize handling.
    _resize_callback: Option<Closure<dyn FnMut()>>,
    listener_generation: Option<Rc<RefCell<Option<u64>>>>,
}

#[derive(Props, Clone, PartialEq)]
pub struct MobileXtermMountProps {
    pub pane_id: String,
    pub cwd: String,
}

#[component]
pub fn MobileXtermMount(props: MobileXtermMountProps) -> Element {
    let pane_id = props.pane_id.clone();
    let mount_id = format!("mobile-xterm-{}", pane_id);
    let cwd = props.cwd.clone();
    let pane_id_for_drop = pane_id.clone();
    let listener_owner: Rc<String> =
        use_hook(|| Rc::new(format!("mobile:{}", js_sys::Math::random())));
    let listener_owner_for_effect = listener_owner.clone();
    let mount_id_for_effect = mount_id.clone();
    let mut cleanup = use_signal(|| Option::<MobileXtermCleanup>::None);
    let mounted = use_hook(|| Rc::new(RefCell::new(false)));
    let active = use_hook(|| Rc::new(RefCell::new(true)));
    let active_for_drop = active.clone();
    // Input plumbing lives at component scope so both xterm's onData and the
    // on-screen keybar feed the same batched writer.
    let input_queue: Rc<RefCell<VecDeque<String>>> =
        use_hook(|| Rc::new(RefCell::new(VecDeque::new())));
    let input_draining = use_hook(|| Rc::new(RefCell::new(false)));
    // One-shot Ctrl modifier for the keybar (tap Ctrl, then a key).
    let mut ctrl_armed = use_signal(|| false);
    let ctrl_cell = use_hook(|| Rc::new(std::cell::Cell::new(false)));
    // Mirror the Cell so the JS callback path sees taps (plain comment —
    // `///` on a statement is a lint error).
    #[allow(unused)]
    let _ = ctrl_cell.clone();
    ctrl_cell.set(ctrl_armed());

    let pane_id_for_keybar = pane_id.clone();
    let active_for_keybar = active.clone();
    let input_queue_for_effect = input_queue.clone();
    let input_draining_for_effect = input_draining.clone();
    let ctrl_for_effect = ctrl_cell.clone();
    use_effect(move || {
        if *mounted.borrow() {
            return;
        }
        *mounted.borrow_mut() = true;
        // Re-clone per effect run: the async task below moves these, and the
        // effect closure is FnMut (re-runnable).
        let input_queue_for_effect = input_queue_for_effect.clone();
        let input_draining_for_effect = input_draining_for_effect.clone();
        let ctrl_for_effect = ctrl_for_effect.clone();

        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(container) = document.get_element_by_id(&mount_id_for_effect) else {
            web_sys::console::error_1(&"[mobile xterm] mount container missing".into());
            return;
        };

        if let Ok(Some(_)) = container.query_selector(".xterm") {
            container.set_inner_html("");
        }

        let pane_id_for_task = pane_id.clone();
        let cwd_for_task = if cwd.trim().is_empty() {
            "/tmp".to_string()
        } else {
            cwd.clone()
        };
        let active_for_task = active.clone();
        let window_for_task = window.clone();
        let container_for_task = container.clone();
        let mut cleanup = cleanup;

        let listener_owner_for_task = listener_owner_for_effect.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if !*active_for_task.borrow() {
                return;
            }

            let has_session = tauri_bridge::pty_has_session(&pane_id_for_task)
                .await
                .unwrap_or(false);
            if !has_session {
                let shell = tauri_bridge::pty_default_shell_cached().await;
                if let Err(error) = tauri_bridge::pty_spawn(
                    &pane_id_for_task,
                    &cwd_for_task,
                    &shell,
                    100,
                    28,
                    true,
                    Some(listener_owner_for_task.as_str()),
                )
                .await
                {
                    web_sys::console::error_1(
                        &format!("[mobile xterm] pty_spawn failed: {error:?}").into(),
                    );
                    return;
                }
            }

            // Capture best-effort scrollback before pausing. Prefer the relay's
            // raw replay buffer: exact VT bytes (cursor position, colors, a
            // partial in-flight line) restore true screen state after a
            // reconnect. Fall back to ANSI-stripped text history when no raw
            // replay exists (fresh spawn / nothing flushed yet). Bytes after
            // this snapshot are buffered by the pause handshake and arrive
            // through xterm's raw stream after attach, avoiding a duplicate
            // replay.
            let raw_replay = tauri_bridge::pty_raw_replay(&pane_id_for_task)
                .await
                .ok()
                .flatten()
                .filter(|bytes| !bytes.is_empty());
            let text_history = if raw_replay.is_none() {
                tauri_bridge::output_buffer_get(&pane_id_for_task, Some(240), None)
                    .await
                    .ok()
                    .and_then(|raw| {
                        serde_json::from_str::<Vec<tauri_bridge::OutputLine>>(&raw).ok()
                    })
                    .map(|lines| {
                        lines
                            .into_iter()
                            .map(|line| line.text)
                            .collect::<Vec<_>>()
                            .join("\r\n")
                    })
                    .filter(|text| !text.is_empty())
            } else {
                None
            };

            // Mark the session xterm-managed before installing the raw
            // listener. Over the relay this call is pane-share-gated: while
            // the desktop considers a "Request access" approval, the phone
            // must RETRY instead of permanently stranding the mount. ~20 s of
            // retries covers the human approval window without spinning.
            let mut xterm_claimed = false;
            for attempt in 0..14 {
                if !*active_for_task.borrow() {
                    return;
                }
                match tauri_bridge::pty_set_xterm(&pane_id_for_task, true).await {
                    Ok(()) => {
                        xterm_claimed = true;
                        break;
                    }
                    Err(error) => {
                        if attempt == 0 || attempt % 5 == 4 {
                            web_sys::console::log_1(
                                &format!("[mobile xterm] awaiting pane share: {error:?}").into(),
                            );
                        }
                        gloo::timers::future::TimeoutFuture::new(1_500).await;
                    }
                }
            }
            if xterm_claimed {
                // continue with terminal setup
            } else {
                web_sys::console::warn_1(
                    &"[mobile xterm] pane never became xterm-accessible (share declined?)".into(),
                );
                return;
            }

            let Some(term_ctor) =
                js_sys::Reflect::get(&window_for_task, &JsValue::from_str("Terminal"))
                    .ok()
                    .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
            else {
                web_sys::console::error_1(&"[mobile xterm] Terminal global missing".into());
                return;
            };

            let options = js_sys::Object::new();
            let set = |key: &str, value: JsValue| {
                let _ = js_sys::Reflect::set(&options, &JsValue::from_str(key), &value);
            };
            set("cursorBlink", JsValue::from_bool(true));
            set("convertEol", JsValue::from_bool(false));
            set("scrollback", JsValue::from_f64(10000.0));
            set(
                "fontFamily",
                JsValue::from_str("'JetBrains Mono', 'JetBrainsMono Nerd Font', monospace"),
            );
            set("fontSize", JsValue::from_f64(13.0));
            set("theme", {
                let theme = js_sys::Object::new();
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("background"),
                    &JsValue::from_str("#08090b"),
                );
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("foreground"),
                    &JsValue::from_str("#e7e4dc"),
                );
                let _ = js_sys::Reflect::set(
                    &theme,
                    &JsValue::from_str("cursor"),
                    &JsValue::from_str("#d8a84e"),
                );
                theme.into()
            });

            let Ok(term) = js_sys::Reflect::construct(&term_ctor, &js_sys::Array::of1(&options))
            else {
                web_sys::console::error_1(&"[mobile xterm] Terminal construction failed".into());
                return;
            };
            let Ok(open) = js_sys::Reflect::get(&term, &JsValue::from_str("open"))
                .and_then(|value| value.dyn_into::<js_sys::Function>())
            else {
                return;
            };
            if open.call1(&term, container_for_task.as_ref()).is_err() {
                return;
            }

            let fit_addon = new_addon(&window_for_task, "FitAddon", &term);
            if let Some(addon) = fit_addon.as_ref() {
                fit(addon);
            }

            // GPU renderer: same confidence order as the desktop — WebGL
            // first, xterm's DOM renderer as silent fallback (older WebViews
            // or exhausted GL contexts simply return None here).
            let webgl_addon = new_addon(&window_for_task, "WebglAddon", &term);

            // Replay best-effort scrollback before resuming the raw VT stream.
            // Raw replay (when present) is exact VT state; text history is the
            // ANSI-stripped fallback. Either way this is history, not a claimed
            // full VT snapshot: alternate-screen apps and cursor state are
            // owned by the live PTY.
            if let Some(bytes) = raw_replay {
                write_bytes(&term, &bytes);
            } else if let Some(history) = text_history {
                write_text(&term, &format!("{}\r\n", history));
            }

            if !*active_for_task.borrow() {
                let pane_for_cancel = pane_id_for_task.clone();
                let owner_for_cancel = listener_owner_for_task.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = tauri_bridge::pty_detach_listener(
                        &pane_for_cancel,
                        owner_for_cancel.as_str(),
                        0,
                    )
                    .await;
                });
                return;
            }

            let term_for_output = term.clone();
            let active_for_output = active_for_task.clone();
            let listener =
                match tauri_bridge::pty_listen_raw(&pane_id_for_task, move |bytes: Vec<u8>| {
                    if *active_for_output.borrow() {
                        write_bytes(&term_for_output, &bytes);
                    }
                }) {
                    Ok(listener) => listener,
                    Err(error) => {
                        web_sys::console::error_1(
                            &format!("[mobile xterm] raw listener failed: {error}").into(),
                        );
                        let pane_for_cancel = pane_id_for_task.clone();
                        let owner_for_cancel = listener_owner_for_task.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ = tauri_bridge::pty_detach_listener(
                                &pane_for_cancel,
                                owner_for_cancel.as_str(),
                                0,
                            )
                            .await;
                        });
                        return;
                    }
                };
            let listener_generation = Rc::new(RefCell::new(None));
            let listener_owner_for_attach = listener_owner_for_task.clone();
            let pane_for_attach = pane_id_for_task.clone();
            let generation_for_attach = listener_generation.clone();
            let active_for_attach = active_for_task.clone();
            let unlisten = listener.unlisten;
            let unlisten_raw = listener.unlisten_raw;
            wasm_bindgen_futures::spawn_local(async move {
                for attempt in 0..4 {
                    if !*active_for_attach.borrow() {
                        return;
                    }
                    match tauri_bridge::pty_attach_listener(
                        &pane_for_attach,
                        listener_owner_for_attach.as_str(),
                        has_session,
                    )
                    .await
                    {
                        Ok(generation) if generation != 0 => {
                            *generation_for_attach.borrow_mut() = Some(generation);
                            if !*active_for_attach.borrow() {
                                let _ = tauri_bridge::pty_detach_listener(
                                    &pane_for_attach,
                                    listener_owner_for_attach.as_str(),
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
                            let _ = tauri_bridge::pty_detach_listener(
                                &pane_for_attach,
                                listener_owner_for_attach.as_str(),
                                0,
                            )
                            .await;
                            return;
                        }
                        Err(_) if attempt < 3 => {
                            gloo::timers::future::TimeoutFuture::new(25).await;
                        }
                        Err(_) => {
                            let _ = tauri_bridge::pty_detach_listener(
                                &pane_for_attach,
                                listener_owner_for_attach.as_str(),
                                0,
                            )
                            .await;
                            return;
                        }
                    }
                }
            });

            let on_data_queue = input_queue_for_effect.clone();
            let on_data_draining = input_draining_for_effect.clone();
            let on_data_active = active_for_task.clone();
            let pane_for_data = pane_id_for_task.clone();
            let ctrl_for_data = ctrl_for_effect.clone();
            let on_data = Closure::wrap(Box::new(move |data: String| {
                // Keybar Ctrl is a one-shot modifier: when armed, translate a
                // single character to its control byte (c → 0x03) and disarm.
                let data = if ctrl_for_data.get() {
                    ctrl_for_data.set(false);
                    ctrl_modified(&data).unwrap_or(data)
                } else {
                    data
                };
                enqueue_input(
                    &on_data_queue,
                    &on_data_draining,
                    &on_data_active,
                    &pane_for_data,
                    data,
                );
            }) as Box<dyn FnMut(String)>);
            if let Ok(on_data_fn) = js_sys::Reflect::get(&term, &JsValue::from_str("onData"))
                .and_then(|value| value.dyn_into::<js_sys::Function>())
            {
                let _ = on_data_fn.call1(&term, on_data.as_ref());
            }
            let on_data_js = on_data.into_js_value();

            let pane_for_resize = pane_id_for_task.clone();
            let on_resize = Closure::wrap(Box::new(move |event: JsValue| {
                let cols = js_sys::Reflect::get(&event, &JsValue::from_str("cols"))
                    .ok()
                    .and_then(|value| value.as_f64())
                    .unwrap_or(80.0) as u16;
                let rows = js_sys::Reflect::get(&event, &JsValue::from_str("rows"))
                    .ok()
                    .and_then(|value| value.as_f64())
                    .unwrap_or(24.0) as u16;
                let pane = pane_for_resize.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = tauri_bridge::pty_resize(&pane, cols.max(1), rows.max(1), None).await;
                });
            }) as Box<dyn FnMut(JsValue)>);
            if let Ok(on_resize_fn) = js_sys::Reflect::get(&term, &JsValue::from_str("onResize"))
                .and_then(|value| value.dyn_into::<js_sys::Function>())
            {
                let _ = on_resize_fn.call1(&term, on_resize.as_ref());
            }
            let on_resize_js = on_resize.into_js_value();

            let mut resize_observer = None;
            let mut resize_callback = None;
            if let Some(addon) = fit_addon {
                let fit_for_resize = addon.clone();
                let callback =
                    Closure::wrap(Box::new(move || fit(&fit_for_resize)) as Box<dyn FnMut()>);
                let observer =
                    js_sys::Reflect::get(&window_for_task, &JsValue::from_str("ResizeObserver"))
                        .ok()
                        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
                        .and_then(|ctor| {
                            js_sys::Reflect::construct(
                                &ctor,
                                &js_sys::Array::of1(callback.as_ref()),
                            )
                            .ok()
                        });
                if let Some(observer) = observer {
                    if let Ok(observe) =
                        js_sys::Reflect::get(&observer, &JsValue::from_str("observe"))
                            .and_then(|value| value.dyn_into::<js_sys::Function>())
                    {
                        let _ = observe.call1(&observer, container_for_task.as_ref());
                        resize_observer = Some(observer);
                        resize_callback = Some(callback);
                    }
                }
            }

            cleanup.set(Some(MobileXtermCleanup {
                term,
                unlisten,
                unlisten_raw,
                _webgl: webgl_addon,
                _on_data: on_data_js,
                _on_resize: on_resize_js,
                resize_observer,
                _resize_callback: resize_callback,
                listener_generation: Some(listener_generation),
            }));
        });
    });

    use_drop(move || {
        *active_for_drop.borrow_mut() = false;
        if let Some(mut mounted) = cleanup.take() {
            let generation = mounted
                .listener_generation
                .as_ref()
                .and_then(|generation| *generation.borrow())
                .unwrap_or(0);
            let pane_for_detach = pane_id_for_drop.clone();
            let owner_for_detach = listener_owner.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = tauri_bridge::pty_detach_listener(
                    &pane_for_detach,
                    owner_for_detach.as_str(),
                    generation,
                )
                .await;
            });

            if let Some(unlisten) = mounted.unlisten.take() {
                unlisten();
            }
            if let Some(unlisten_raw) = mounted.unlisten_raw.take() {
                unlisten_raw();
            }
            if let Some(observer) = mounted.resize_observer.take() {
                if let Ok(disconnect) =
                    js_sys::Reflect::get(&observer, &JsValue::from_str("disconnect"))
                        .and_then(|value| value.dyn_into::<js_sys::Function>())
                {
                    let _ = disconnect.call0(&observer);
                }
            }
            if let Ok(dispose) = js_sys::Reflect::get(&mounted.term, &JsValue::from_str("dispose"))
                .and_then(|value| value.dyn_into::<js_sys::Function>())
            {
                let _ = dispose.call0(&mounted.term);
            }
        } else {
            // The mount may be dropped before xterm setup stores cleanup.
            // Cancel the generation-zero startup lease so a delayed attach
            // from this abandoned owner cannot claim a replacement PTY.
            let pane_for_cancel = pane_id_for_drop.clone();
            let owner_for_cancel = listener_owner.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = tauri_bridge::pty_detach_listener(
                    &pane_for_cancel,
                    owner_for_cancel.as_str(),
                    0,
                )
                .await;
            });
        }
    });

    // Shared send closure for the on-screen keybar: Esc/Tab/arrows and the
    // Ctrl one-shot modifier all feed the same batched input queue as xterm's
    // onData (hardware/soft keyboard input).
    let send_key = {
        let queue = input_queue.clone();
        let draining = input_draining.clone();
        let active_for_keybar = active_for_keybar.clone();
        let pane_id_for_keybar = pane_id_for_keybar.clone();
        Rc::new(move |data: &str| {
            enqueue_input(
                &queue,
                &draining,
                &active_for_keybar,
                &pane_id_for_keybar,
                data.to_string(),
            );
        })
    };

    let ctrl_on = ctrl_armed();
    rsx! {
        div { class: "mobile-xterm-wrap",
            div { class: "mobile-keybar", role: "toolbar", aria_label: "Terminal keys",
                button {
                    class: "mobile-keybar-key",
                    // Keep focus in the terminal; don't let the tap blur xterm.
                    onmousedown: move |e| e.prevent_default(),
                    onclick: {
                        let send = send_key.clone();
                        move |_| send("\x1b")
                    },
                    "esc"
                }
                button {
                    class: "mobile-keybar-key",
                    onmousedown: move |e| e.prevent_default(),
                    onclick: {
                        let send = send_key.clone();
                        move |_| send("\t")
                    },
                    "tab"
                }
                button {
                    class: if ctrl_on { "mobile-keybar-key is-armed" } else { "mobile-keybar-key" },
                    onmousedown: move |e| e.prevent_default(),
                    onclick: move |_| {
                        let next = !ctrl_armed();
                        ctrl_armed.set(next);
                        ctrl_cell.set(next);
                    },
                    "ctrl"
                }
                for (label, seq) in [("←", "\x1b[D"), ("↓", "\x1b[B"), ("↑", "\x1b[A"), ("→", "\x1b[C")] {
                    button {
                        key: "key-{label}",
                        class: "mobile-keybar-key",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: {
                            let send = send_key.clone();
                            move |_| send(seq)
                        },
                        "{label}"
                    }
                }
                for (label, seq) in [("-", "-"), ("_", "_"), ("|", "|"), ("~", "~"), ("/", "/")] {
                    button {
                        key: "key-{label}",
                        class: "mobile-keybar-key",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: {
                            let send = send_key.clone();
                            move |_| send(seq)
                        },
                        "{label}"
                    }
                }
            }
            div {
                id: "{mount_id}",
                class: "mobile-xterm-mount",
                role: "application",
                aria_label: "Live terminal",
            }
        }
    }
}
