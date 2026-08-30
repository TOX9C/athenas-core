//! Browser and xterm.js helpers shared by the mount lifecycle.

use crate::stores::terminal::TerminalSession;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

pub(crate) fn read_css_var(window: &web_sys::Window, name: &str) -> String {
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

/// Activate the vendored `@xterm/addon-web-links` addon with a custom handler.
///
/// The addon's default handler opens `window.open()` (a popup / new native
/// window), which is not what this app wants. Passing our own `handler`
/// (a `JsValue` wrapping a Rust `Closure<dyn FnMut(JsValue, String)>`) routes
/// link clicks into the embedded browser panel instead. The caller must keep
/// `handler` rooted until `term.dispose()` runs.
pub(crate) fn try_activate_web_links_addon(
    window: &web_sys::Window,
    term_val: &JsValue,
    handler: &JsValue,
) -> Option<JsValue> {
    let ctor_val = js_sys::Reflect::get(window, &JsValue::from_str("WebLinksAddon")).ok()?;
    let ctor_val = if ctor_val.is_function() {
        ctor_val
    } else {
        js_sys::Reflect::get(&ctor_val, &JsValue::from_str("WebLinksAddon")).ok()?
    };
    let ctor: js_sys::Function = ctor_val.dyn_into().ok()?;
    let instance = js_sys::Reflect::construct(&ctor, &js_sys::Array::of1(handler)).ok()?;
    let activate_val = js_sys::Reflect::get(&instance, &JsValue::from_str("activate")).ok()?;
    let activate_fn: js_sys::Function = activate_val.dyn_into().ok()?;
    let _ = activate_fn.call1(&instance, term_val);
    Some(instance)
}

pub(crate) fn try_activate_addon(
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

pub(crate) fn write_bytes_to_term(term_val: &JsValue, bytes: &[u8]) {
    let Ok(write_val) = js_sys::Reflect::get(term_val, &JsValue::from_str("write")) else {
        return;
    };
    let Ok(write_fn) = write_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let bytes_arr = js_sys::Uint8Array::from(bytes);
    let _ = write_fn.call1(term_val, bytes_arr.as_ref());
}

pub(crate) fn write_str_to_term(term_val: &JsValue, text: &str) {
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
pub(crate) fn serialize_buffer(serialize_addon: &JsValue) -> Option<String> {
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

pub(crate) fn restore_term_from_session(term_val: &JsValue, session: &TerminalSession) {
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

/// Viewport intent captured before a geometry change. `at_bottom` means the
/// user was following the newest output; otherwise we preserve the distance
/// from the bottom instead of unexpectedly forcing the terminal to the prompt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewportState {
    pub at_bottom: bool,
    pub distance_from_bottom: i32,
}

fn read_number(value: &JsValue, property: &str) -> Option<i32> {
    js_sys::Reflect::get(value, &JsValue::from_str(property))
        .ok()
        .and_then(|value| value.as_f64())
        .map(|value| value.round() as i32)
}

fn active_buffer(term_val: &JsValue) -> Option<JsValue> {
    let buffer = js_sys::Reflect::get(term_val, &JsValue::from_str("buffer")).ok()?;
    js_sys::Reflect::get(&buffer, &JsValue::from_str("active")).ok()
}

/// Capture xterm's visible position before FitAddon changes the row count.
///
/// xterm.js exposes `buffer.active.viewportY` and `baseY` specifically for
/// inspecting the visible viewport. Keeping this intent separate from the
/// serialized terminal contents lets normal-buffer scrollback and alternate
/// screen applications follow their own redraw semantics.
pub(crate) fn capture_viewport(term_val: &JsValue) -> ViewportState {
    let Some(active) = active_buffer(term_val) else {
        return ViewportState {
            at_bottom: true,
            distance_from_bottom: 0,
        };
    };
    let viewport_y = read_number(&active, "viewportY").unwrap_or(0).max(0);
    let base_y = read_number(&active, "baseY").unwrap_or(viewport_y).max(0);
    ViewportState {
        at_bottom: viewport_y >= base_y,
        distance_from_bottom: base_y.saturating_sub(viewport_y),
    }
}

/// Restore the visible position after xterm has applied a new geometry.
pub(crate) fn restore_viewport(term_val: &JsValue, state: ViewportState) {
    let method = if state.at_bottom {
        "scrollToBottom"
    } else {
        "scrollToLine"
    };
    let Ok(method_val) = js_sys::Reflect::get(term_val, &JsValue::from_str(method)) else {
        return;
    };
    let Ok(method_fn) = method_val.dyn_into::<js_sys::Function>() else {
        return;
    };

    if state.at_bottom {
        let _ = method_fn.call0(term_val);
        return;
    }

    let target_line = active_buffer(term_val)
        .and_then(|active| read_number(&active, "baseY"))
        .unwrap_or(0)
        .saturating_sub(state.distance_from_bottom)
        .max(0);
    let _ = method_fn.call1(term_val, &JsValue::from_f64(target_line as f64));
}

// FitAddon recomputes xterm's rows/columns from the container and the
// renderer's measured cell dimensions. Repainting in the same callback is
// unsafe in WebKit: CanvasAddon may still be applying the new canvas size.
// Keep this function focused on fitting; `schedule_fit` performs the repaint
// and viewport restoration on the following animation frame.
pub(crate) fn call_fit(fit_instance: &JsValue, container: &web_sys::Element, _term_val: &JsValue) {
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

pub(crate) fn schedule_fit(
    window: &web_sys::Window,
    fit_instance: &JsValue,
    container: &web_sys::Element,
    term_val: &JsValue,
    pending: &Rc<RefCell<bool>>,
    active: &Rc<RefCell<bool>>,
    viewport: &Rc<RefCell<Option<ViewportState>>>,
) {
    // ResizeObserver, font updates, visibility restoration, and pane swaps can
    // all request a fit in the same frame. Keep the whole fit/repaint pair in
    // flight so xterm does not resize/repaint repeatedly while WebKit is still
    // settling flex layout. Capture only the first request in a coalesced burst.
    if !*active.borrow() || *pending.borrow() {
        return;
    }
    *viewport.borrow_mut() = Some(capture_viewport(term_val));
    *pending.borrow_mut() = true;

    let fit_for_raf = fit_instance.clone();
    let container_for_raf = container.clone();
    let term_for_fit = term_val.clone();
    let term_for_refresh = term_val.clone();
    let pending_for_raf = pending.clone();
    let active_for_raf = active.clone();
    let viewport_for_raf = viewport.clone();
    let window_for_refresh = window.clone();
    // The first RAF commits the new xterm grid dimensions. The second RAF
    // lets xterm commit its resized buffer before repainting and restoring the
    // viewport intent. This also gives full-screen applications one frame to
    // process the PTY's SIGWINCH redraw.
    let raf_closure = wasm_bindgen::closure::Closure::once_into_js(move || {
        if !*active_for_raf.borrow() {
            *pending_for_raf.borrow_mut() = false;
            *viewport_for_raf.borrow_mut() = None;
            return;
        }
        call_fit(&fit_for_raf, &container_for_raf, &term_for_fit);

        let pending_for_refresh = pending_for_raf.clone();
        let active_for_refresh = active_for_raf.clone();
        let viewport_for_refresh = viewport_for_raf.clone();
        let pending_for_refresh_fallback = pending_for_refresh.clone();
        let active_for_refresh_fallback = active_for_refresh.clone();
        let viewport_for_refresh_fallback = viewport_for_refresh.clone();
        let term_for_refresh_fallback = term_for_refresh.clone();
        let refresh_closure = wasm_bindgen::closure::Closure::once_into_js(move || {
            if *active_for_refresh.borrow() {
                refresh_full(&term_for_refresh);
                if let Some(state) = viewport_for_refresh.borrow_mut().take() {
                    restore_viewport(&term_for_refresh, state);
                }
            } else {
                *viewport_for_refresh.borrow_mut() = None;
            }
            *pending_for_refresh.borrow_mut() = false;
        });
        if window_for_refresh
            .request_animation_frame(refresh_closure.as_ref().unchecked_ref())
            .is_err()
        {
            if *active_for_refresh_fallback.borrow() {
                refresh_full(&term_for_refresh_fallback);
                if let Some(state) = viewport_for_refresh_fallback.borrow_mut().take() {
                    restore_viewport(&term_for_refresh_fallback, state);
                }
            } else {
                *viewport_for_refresh_fallback.borrow_mut() = None;
            }
            *pending_for_refresh_fallback.borrow_mut() = false;
        }
    });
    if window
        .request_animation_frame(raf_closure.as_ref().unchecked_ref())
        .is_err()
    {
        call_fit(fit_instance, container, term_val);
        refresh_full(term_val);
        if let Some(state) = viewport.borrow_mut().take() {
            restore_viewport(term_val, state);
        }
        *pending.borrow_mut() = false;
    }
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
pub(crate) fn refresh_full(term_val: &JsValue) {
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
pub(crate) fn force_redraw(term_val: &JsValue) {
    refresh_full(term_val);
}

// ---------------------------------------------------------------------------
// Wait until the container has a non-zero size before opening xterm.
// On remount after a pane swap, the flex grid may not have laid out yet,
// so the container rect can be 0×0. Polling with RAF gives the browser a
// chance to reflow. Capped at ~300ms to avoid hanging indefinitely.
// ---------------------------------------------------------------------------
#[inline]
pub(crate) fn is_container_sized(width: f64, height: f64) -> bool {
    width > 0.0 && height > 0.0
}

/// xterm's FitAddon never proposes fewer than two columns and one row. Keep
/// invalid resize events out of the PTY resize queue so a transient zero-sized
/// or partially-laid-out pane cannot desynchronize the shell dimensions.
pub(crate) fn is_valid_terminal_dimensions(cols: u16, rows: u16) -> bool {
    cols >= 2 && rows >= 1
}

/// Wait for the selected web font to be loaded before xterm measures its cell
/// geometry. `document.fonts.ready` alone does not necessarily request a font
/// that is only referenced by Terminal.options, so explicitly call
/// FontFaceSet.load first when the browser exposes it.
pub(crate) async fn wait_for_font_ready(
    window: &web_sys::Window,
    font_family: &str,
    font_size: f64,
) {
    let Some(document) = window.document() else {
        return;
    };
    let Ok(fonts) = js_sys::Reflect::get(&document, &JsValue::from_str("fonts")) else {
        return;
    };
    if fonts.is_undefined() || fonts.is_null() {
        return;
    }

    if !font_family.trim().is_empty() {
        if let Ok(load_val) = js_sys::Reflect::get(&fonts, &JsValue::from_str("load")) {
            if let Ok(load_fn) = load_val.dyn_into::<js_sys::Function>() {
                let font_spec =
                    JsValue::from_str(&format!("{}px {}", font_size.max(1.0), font_family.trim()));
                if let Ok(promise) = load_fn.call1(&fonts, &font_spec) {
                    let _ = JsFuture::from(js_sys::Promise::from(promise)).await;
                }
            }
        }
    }

    if let Ok(ready_val) = js_sys::Reflect::get(&fonts, &JsValue::from_str("ready")) {
        if ready_val.is_object() {
            let _ = JsFuture::from(js_sys::Promise::from(ready_val)).await;
        }
    }
}

const CONTAINER_SIZE_RETRIES: usize = 15;

enum ContainerSizePoll {
    Ready,
    Retry,
    Exhausted,
}

#[inline]
fn poll_container_size(width: f64, height: f64, attempt: usize) -> ContainerSizePoll {
    if is_container_sized(width, height) {
        ContainerSizePoll::Ready
    } else if attempt + 1 < CONTAINER_SIZE_RETRIES {
        ContainerSizePoll::Retry
    } else {
        ContainerSizePoll::Exhausted
    }
}

pub(crate) async fn wait_for_container_size(container: &web_sys::Element) {
    for attempt in 0..CONTAINER_SIZE_RETRIES {
        let rect = container.get_bounding_client_rect();
        match poll_container_size(rect.width(), rect.height(), attempt) {
            ContainerSizePoll::Ready => return,
            ContainerSizePoll::Exhausted => break,
            ContainerSizePoll::Retry => {}
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

#[cfg(test)]
mod tests {
    use super::{
        is_container_sized, is_valid_terminal_dimensions, poll_container_size, ContainerSizePoll,
        CONTAINER_SIZE_RETRIES,
    };

    #[test]
    fn terminal_dimensions_require_a_real_xterm_grid() {
        assert!(is_valid_terminal_dimensions(2, 1));
        assert!(is_valid_terminal_dimensions(80, 24));
        assert!(!is_valid_terminal_dimensions(1, 24));
        assert!(!is_valid_terminal_dimensions(80, 0));
    }

    #[test]
    fn terminal_dimensions_reject_zero_resize_fallbacks() {
        assert!(!is_valid_terminal_dimensions(0, 0));
        assert!(!is_valid_terminal_dimensions(0, 24));
        assert!(!is_valid_terminal_dimensions(80, 0));
    }

    #[test]
    fn container_size_retry_budget_is_bounded() {
        assert_eq!(CONTAINER_SIZE_RETRIES, 15);
    }

    #[test]
    fn container_size_poll_stops_on_first_sized_layout() {
        assert!(matches!(
            poll_container_size(100.0, 40.0, 0),
            ContainerSizePoll::Ready
        ));
    }

    #[test]
    fn container_size_poll_retries_before_the_budget_is_exhausted() {
        assert!(matches!(
            poll_container_size(0.0, 0.0, CONTAINER_SIZE_RETRIES - 2),
            ContainerSizePoll::Retry
        ));
    }

    #[test]
    fn container_size_poll_allows_bounded_fallback_after_the_last_attempt() {
        assert!(matches!(
            poll_container_size(0.0, 0.0, CONTAINER_SIZE_RETRIES - 1),
            ContainerSizePoll::Exhausted
        ));
    }

    #[test]
    fn container_is_sized_only_when_both_dimensions_are_positive() {
        assert!(is_container_sized(1.0, 1.0));
        assert!(!is_container_sized(0.0, 100.0));
        assert!(!is_container_sized(100.0, 0.0));
        assert!(!is_container_sized(-1.0, 100.0));
        assert!(!is_container_sized(100.0, -1.0));
    }
}

// scan_for_resume_id has been replaced by ResumeScanner in utils::resume_scanner.
// Kept out to prevent stale references.
