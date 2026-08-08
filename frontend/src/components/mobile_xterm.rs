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
        
            let next = queue.borrow_mut().pop_front();
            let Some(data) = next else {
                *draining.borrow_mut() = false;
                return;
            };
            if let Err(error) = tauri_bridge::pty_write(&pane_id, &data).await {
                web_sys::console::error_1(
                    &format!("[mobile xterm] pty_write failed: {error:?}").into(),
                );
            }
        }
    });
}

struct MobileXtermCleanup {
    term: JsValue,
    unlisten: Option<Box<dyn FnOnce()>>,
    on_data: JsValue,
    on_resize: JsValue,
    resize_observer: Option<JsValue>,
    resize_callback: Option<Closure<dyn FnMut()>>,
    active: Rc<RefCell<bool>>,
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
    let mount_id_for_effect = mount_id.clone();
    let mut cleanup = use_signal(|| Option::<MobileXtermCleanup>::None);
    let mounted = use_hook(|| Rc::new(RefCell::new(false)));
    let active = use_hook(|| Rc::new(RefCell::new(true)));
    let active_for_drop = active.clone();

    use_effect(move || {
        if *mounted.borrow() {
            return;
        }
        *mounted.borrow_mut() = true;

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
                )
                .await
                {
                    web_sys::console::error_1(
                        &format!("[mobile xterm] pty_spawn failed: {error:?}").into(),
                    );
                    return;
                }
            }

            // Capture best-effort scrollback before pausing. The PTY keeps
            // emitting while this snapshot is read; bytes after the snapshot
            // are buffered by the pause handshake and arrive through xterm's
            // raw stream after attach, avoiding a duplicate replay.
            let history = tauri_bridge::output_buffer_get(&pane_id_for_task, Some(240), None)
                .await
                .ok()
                .and_then(|raw| serde_json::from_str::<Vec<tauri_bridge::OutputLine>>(&raw).ok())
                .map(|lines| {
                    lines
                        .into_iter()
                        .map(|line| line.text)
                        .collect::<Vec<_>>()
                        .join("\r\n")
                })
                .filter(|text| !text.is_empty());

            // Stop the PTY read loop from emitting while the xterm listener is
            // installed. The attach call below resumes it after the snapshot
            // is written, closing the bootstrap gap without polling.
            // The relay exposes these lifecycle commands as part of the
            // authenticated xterm attachment handshake. If either call fails,
            // do not leave the PTY paused.
            if tauri_bridge::pty_set_raw_paused(&pane_id_for_task, true).await.is_err()
                || tauri_bridge::pty_set_xterm(&pane_id_for_task, true).await.is_err()
            {
                let _ = tauri_bridge::pty_set_raw_paused(&pane_id_for_task, false).await;
                return;
            }

            let Some(term_ctor) = js_sys::Reflect::get(
                &window_for_task,
                &JsValue::from_str("Terminal"),
            )
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok()) else {
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
            set("fontFamily", JsValue::from_str("'JetBrains Mono', monospace"));
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

            // Replay best-effort scrollback before resuming the raw VT stream.
            // This is intentionally history, not a claimed full VT snapshot:
            // alternate-screen apps and cursor state are owned by the live PTY.
            if let Some(history) = history {
                write_text(&term, &format!("{}\r\n", history));
            }

            if !*active_for_task.borrow() {
                return;
            }

            let term_for_output = term.clone();
            let active_for_output = active_for_task.clone();
            let unlisten = match tauri_bridge::pty_listen_raw(
                &pane_id_for_task,
                move |bytes: Vec<u8>| {
                    if *active_for_output.borrow() {
                        write_bytes(&term_for_output, &bytes);
                    }
                },
            ) {
                Ok(unlisten) => unlisten,
                Err(error) => {
                    web_sys::console::error_1(
                        &format!("[mobile xterm] raw listener failed: {error}").into(),
                    );
                    return;
                }
            };
            let pane_for_attach = pane_id_for_task.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = tauri_bridge::pty_attach_listener(&pane_for_attach).await;
            });

            let input_queue = Rc::new(RefCell::new(VecDeque::<String>::new()));
            let input_draining = Rc::new(RefCell::new(false));
            let on_data_queue = input_queue.clone();
            let on_data_draining = input_draining.clone();
            let on_data_active = active_for_task.clone();
            let pane_for_data = pane_id_for_task.clone();
            let on_data = Closure::wrap(Box::new(move |data: String| {
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
                    let _ = tauri_bridge::pty_resize(&pane, cols.max(1), rows.max(1)).await;
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
                let callback = Closure::wrap(Box::new(move || fit(&fit_for_resize)) as Box<dyn FnMut()>);
                let observer = js_sys::Reflect::get(
                    &window_for_task,
                    &JsValue::from_str("ResizeObserver"),
                )
                .ok()
                .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
                .and_then(|ctor| {
                    js_sys::Reflect::construct(&ctor, &js_sys::Array::of1(callback.as_ref())).ok()
                });
                if let Some(observer) = observer {
                    if let Ok(observe) = js_sys::Reflect::get(&observer, &JsValue::from_str("observe"))
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
                unlisten: Some(unlisten),
                on_data: on_data_js,
                on_resize: on_resize_js,
                resize_observer,
                resize_callback,
                active: active_for_task,
            }));
        });
    });

    use_drop(move || {
        *active_for_drop.borrow_mut() = false;
        let pane_for_pause = pane_id_for_drop.clone();
        wasm_bindgen_futures::spawn_local(async move {
            // Pause before removing the listener. A following mount performs
            // the attach handshake after it has installed its listener.
            let _ = tauri_bridge::pty_set_raw_paused(&pane_for_pause, true).await;
        });
        if let Some(mut mounted) = cleanup.take() {
            if let Some(unlisten) = mounted.unlisten.take() {
                unlisten();
            }
            if let Some(observer) = mounted.resize_observer.take() {
                if let Ok(disconnect) = js_sys::Reflect::get(&observer, &JsValue::from_str("disconnect"))
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
        }
    });

    rsx! {
        div {
            id: "{mount_id}",
            class: "mobile-xterm-mount",
            role: "application",
            aria_label: "Live terminal",
        }
    }
}
