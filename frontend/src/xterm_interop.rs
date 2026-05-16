use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::Element;

/// Global JS terminal store -- mirrors the JS-side Map managed by the inline scripts
/// that are loaded in index.html. We access them via js_sys.
///
/// Initialize an xterm.js Terminal instance inside the given DOM element.
/// Returns an opaque string handle (key into the JS-side Map) for subsequent operations.
pub fn create_terminal(container: &Element, theme: &str) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;

    // Call window.__athenaCreateTerminal(container, theme)
    let fn_name = JsValue::from_str("__athenaCreateTerminal");
    let global_fn = js_sys::Reflect::get(&window, &fn_name)?;
    let global_fn = global_fn.dyn_into::<js_sys::Function>()?;

    let result = global_fn.call2(&window, container, &JsValue::from_str(theme))?;
    // The JS function returns a string handle (Map key)
    Ok(result
        .as_string()
        .unwrap_or_else(|| result.as_f64().unwrap_or(0.0).to_string()))
}

/// Write data (including ANSI sequences) to the terminal.
pub fn write_terminal(handle: &str, data: &str) {
    call_global_1("__athenaWriteTerminal", handle, &JsValue::from_str(data));
}

/// Register an onData callback (user keystrokes).
pub fn on_terminal_data(handle: &str, callback: impl Fn(String) + 'static) {
    let closure = Closure::wrap(Box::new(move |data: JsValue| {
        if let Some(s) = data.as_string() {
            callback(s);
        }
    }) as Box<dyn Fn(JsValue)>);

    if let Some(window) = web_sys::window() {
        if let Ok(fn_val) =
            js_sys::Reflect::get(&window, &JsValue::from_str("__athenaOnTerminalData"))
        {
            if let Ok(global_fn) = fn_val.dyn_into::<js_sys::Function>() {
                let _ = global_fn.call2(&window, &JsValue::from_str(handle), closure.as_ref());
            }
        }
    }
    closure.forget();
}

/// Fit the terminal to its container.
pub fn fit_terminal(handle: &str) {
    call_global_0("__athenaFitTerminal", handle);
}

/// Dispose the terminal instance.
pub fn dispose_terminal(handle: &str) {
    call_global_0("__athenaDisposeTerminal", handle);
}

/// Get the current terminal dimensions as (cols, rows).
pub fn get_terminal_size(handle: &str) -> (u16, u16) {
    if let Some(window) = web_sys::window() {
        if let Ok(fn_val) =
            js_sys::Reflect::get(&window, &JsValue::from_str("__athenaGetTerminalSize"))
        {
            if let Ok(global_fn) = fn_val.dyn_into::<js_sys::Function>() {
                if let Ok(result) = global_fn.call1(&window, &JsValue::from_str(handle)) {
                    if let Ok(arr) = result.dyn_into::<js_sys::Array>() {
                        let cols = arr.get(0).as_f64().unwrap_or(80.0) as u16;
                        let rows = arr.get(1).as_f64().unwrap_or(24.0) as u16;
                        return (cols, rows);
                    }
                }
            }
        }
    }
    (80, 24)
}

/// Resize the terminal to the given dimensions.
pub fn resize_terminal(handle: &str, cols: u16, rows: u16) {
    if let Some(window) = web_sys::window() {
        if let Ok(fn_val) =
            js_sys::Reflect::get(&window, &JsValue::from_str("__athenaResizeTerminal"))
        {
            if let Ok(global_fn) = fn_val.dyn_into::<js_sys::Function>() {
                let _ = global_fn.call3(
                    &window,
                    &JsValue::from_str(handle),
                    &JsValue::from_f64(cols as f64),
                    &JsValue::from_f64(rows as f64),
                );
            }
        }
    }
}

/// Attach a custom keydown event handler to the terminal that intercepts
/// modifier-key combinations (Cmd/Ctrl, Alt, Escape) so they bubble up to
/// the Dioxus root handler instead of being consumed by xterm.js.
///
/// Returns `true` for plain keypresses (no modifiers) so xterm handles
/// them normally (terminal input). Returns `false` for:
/// - Any event with `metaKey` (Cmd on macOS, Win on Linux)
/// - Any event with `ctrlKey`
/// - Any event with `altKey`
/// - The Escape key
pub fn attach_custom_key_event_handler(handle: &str) {
    let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        // Allow Escape to pass through regardless of modifiers
        if event.key() == "Escape" {
            return false;
        }
        // Let modifier combos (Cmd+K, Cmd+J, Ctrl+C, Alt+Backtick, etc.)
        // bubble up to the Dioxus root onkeydown handler.
        if event.meta_key() || event.ctrl_key() || event.alt_key() {
            return false;
        }
        // Regular keypresses (no modifiers) are handled by xterm.js
        // and piped to the PTY via the onData callback.
        true
    }) as Box<dyn Fn(web_sys::KeyboardEvent) -> bool>);

    if let Some(window) = web_sys::window() {
        if let Ok(fn_val) = js_sys::Reflect::get(
            &window,
            &JsValue::from_str("__athenaAttachCustomKeyEventHandler"),
        ) {
            if let Ok(global_fn) = fn_val.dyn_into::<js_sys::Function>() {
                let _ = global_fn.call2(
                    &window,
                    &JsValue::from_str(handle),
                    closure.as_ref(),
                );
            }
        }
    }
    closure.forget();
}

/// Inject the xterm.js bootstrap script into the page if not already present.
/// Call this once on app startup.
pub fn ensure_xterm_bootstrap() {
    if let Some(window) = web_sys::window() {
        if js_sys::Reflect::get(&window, &JsValue::from_str("__athenaCreateTerminal")).is_ok() {
            return; // Already injected
        }
    }

    let script = include_str!("xterm_bootstrap.js");
    if let Some(window) = web_sys::window() {
        if let Some(doc) = window.document() {
            let script_el = doc.create_element("script").expect("create script");
            script_el.set_inner_html(script);
            if let Some(body) = doc.body() {
                let _ = body.append_child(&script_el);
            }
        }
    }
}

fn call_global_0(name: &str, handle: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(fn_val) = js_sys::Reflect::get(&window, &JsValue::from_str(name)) {
            if let Ok(global_fn) = fn_val.dyn_into::<js_sys::Function>() {
                let _ = global_fn.call1(&window, &JsValue::from_str(handle));
            }
        }
    }
}

fn call_global_1(name: &str, handle: &str, arg: &JsValue) {
    if let Some(window) = web_sys::window() {
        if let Ok(fn_val) = js_sys::Reflect::get(&window, &JsValue::from_str(name)) {
            if let Ok(global_fn) = fn_val.dyn_into::<js_sys::Function>() {
                let _ = global_fn.call2(&window, &JsValue::from_str(handle), arg);
            }
        }
    }
}
