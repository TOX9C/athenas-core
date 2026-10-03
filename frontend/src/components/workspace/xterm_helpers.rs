//! Browser and xterm.js helpers shared by the mount lifecycle.

use crate::stores::terminal::TerminalSession;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// How long after a fit the write path keeps re-pinning a bottom-following
/// pane to the bottom. The fit itself fires xterm's `onResize` → `pty_resize`
/// → SIGWINCH, and full-screen TUIs (OMP, vim) redraw asynchronously — often
/// 100–500 ms later — reprinting their whole scrollback. A one-shot viewport
/// restore runs before that burst, so without a follow window the pane can
/// stick at a mid-buffer position indefinitely (the "scrolled up" bug).
pub(crate) const FOLLOW_BOTTOM_WINDOW_MS: f64 = 3200.0;

/// A viewport observed at the bottom within this window still counts as
/// bottom-following when a fit captures its intent. Redraw bursts can leave
/// `viewportY < baseY` for a frame or two; those panes must still follow.
pub(crate) const RECENT_BOTTOM_WINDOW_MS: f64 = 1500.0;

/// True when xterm's active buffer viewport is pinned to the bottom.
pub(crate) fn viewport_at_bottom(term_val: &JsValue) -> bool {
    let Some(active) = active_buffer(term_val) else {
        return true;
    };
    let viewport_y = read_number(&active, "viewportY").unwrap_or(0);
    let base_y = read_number(&active, "baseY").unwrap_or(viewport_y);
    viewport_y >= base_y
}

/// Scroll the terminal viewport to the bottom of the active buffer.
/// No-op when the method is unavailable or the pane is already pinned.
pub(crate) fn scroll_to_bottom(term_val: &JsValue) {
    let Ok(method_val) = js_sys::Reflect::get(term_val, &JsValue::from_str("scrollToBottom"))
    else {
        return;
    };
    let Ok(method_fn) = method_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = method_fn.call0(term_val);
}

/// Record bottom-following intent across a geometry change. A pane that was
/// at the bottom (now, or within [`RECENT_BOTTOM_WINDOW_MS`]) keeps following
/// output for [`FOLLOW_BOTTOM_WINDOW_MS`] after the fit, covering the deferred
/// SIGWINCH redraw burst. Returns true when the pane should follow.
pub(crate) fn note_fit_viewport_intent(
    at_bottom: bool,
    follow_until: &Rc<Cell<f64>>,
    last_bottom_at: &Rc<Cell<f64>>,
) -> bool {
    let now = js_sys::Date::now();
    if at_bottom {
        follow_until.set(now + FOLLOW_BOTTOM_WINDOW_MS);
        last_bottom_at.set(now);
        return true;
    }
    if now - last_bottom_at.get() < RECENT_BOTTOM_WINDOW_MS {
        // Mid-redraw transient: the pane was following moments ago; keep it.
        follow_until.set(now + FOLLOW_BOTTOM_WINDOW_MS);
        return true;
    }
    false
}

/// Re-pin a bottom-following pane across the whole redraw settle window.
///
/// The fit's one-shot viewport restore cannot cover the deferred SIGWINCH
/// redraw: full-screen TUIs (OMP) cycle normal→alt→normal buffers on resize
/// and write their reprint *after* the restore frame — and an idle shell
/// writes nothing at all afterwards, so a write-path re-pin never fires.
/// schedule_fit calls these delayed re-pins whenever follow intent is armed;
/// user scroll gestures cancel them by zeroing `follow_until`.
pub(crate) fn schedule_follow_repins(
    term_val: &JsValue,
    active: &Rc<RefCell<bool>>,
    follow_until: &Rc<Cell<f64>>,
) {
    const REPIN_DELAYS_MS: &[u32] = &[120, 300, 600, 1100, 1800, 2800];
    for &delay in REPIN_DELAYS_MS {
        let term = term_val.clone();
        let active_for_delay = active.clone();
        let follow_for_delay = follow_until.clone();
        wasm_bindgen_futures::spawn_local(async move {
            gloo::timers::future::TimeoutFuture::new(delay).await;
            if !*active_for_delay.borrow() {
                return;
            }
            if js_sys::Date::now() <= follow_for_delay.get() {
                scroll_to_bottom(&term);
            }
        });
    }
}

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

/// Activate the vendored `@xterm/addon-canvas` — the middle renderer tier
/// (WebGL → Canvas → DOM). Same construction shape as the webgl addon but
/// takes no options.
pub(crate) fn try_activate_canvas_addon(
    window: &web_sys::Window,
    term_val: &JsValue,
) -> Option<JsValue> {
    let global_val = js_sys::Reflect::get(window, &JsValue::from_str("CanvasAddon")).ok()?;
    let ctor_val = if global_val.is_function() {
        global_val
    } else {
        js_sys::Reflect::get(&global_val, &JsValue::from_str("CanvasAddon")).ok()?
    };
    if !ctor_val.is_function() {
        return None;
    }
    let ctor: js_sys::Function = ctor_val.dyn_into().ok()?;
    let instance = js_sys::Reflect::construct(&ctor, &js_sys::Array::new()).ok()?;
    let activate_fn: js_sys::Function =
        js_sys::Reflect::get(&instance, &JsValue::from_str("activate")).ok()?.dyn_into().ok()?;
    let _ = activate_fn.call1(&instance, term_val);
    Some(instance)
}

/// Minimal canvas-addon context loss handling: log and dispose so xterm
/// continues with the DOM renderer instead of painting nothing.
pub(crate) fn wire_canvas_context_loss_logging(
    canvas_addon: &JsValue,
) -> Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>> {
    use wasm_bindgen::JsCast;
    let canvas: web_sys::EventTarget =
        js_sys::Reflect::get(canvas_addon, &JsValue::from_str("_renderer"))
            .and_then(|r| js_sys::Reflect::get(&r, &JsValue::from_str("_canvas")))
            .ok()?
            .dyn_into()
            .ok()?;
    let addon_for_loss = canvas_addon.clone();
    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: JsValue| {
        web_sys::console::warn_1(
            &"[XtermMount] Canvas 2D context lost; disposing CanvasAddon, continuing with DOM renderer".into(),
        );
        if let Ok(dispose_fn) =
            js_sys::Reflect::get(&addon_for_loss, &JsValue::from_str("dispose"))
                .and_then(|v| v.dyn_into::<js_sys::Function>())
        {
            let _ = dispose_fn.call0(&addon_for_loss);
        }
    }) as Box<dyn FnMut(JsValue)>);
    let _ = canvas
        .add_event_listener_with_callback("contextlost", closure.as_ref().unchecked_ref());
    Some(closure)
}

/// Activate the vendored `@xterm/addon-webgl` with the DEFAULT drawing-buffer
/// behaviour (`preserveDrawingBuffer: false`).
///
/// Do not turn `preserveDrawingBuffer` on. The vendored renderer sets its only
/// "clear the model" flag (`GlyphRenderer._requestClearModel`) when it grows or
/// merges an atlas PAGE, and for everything else it relies on the driver
/// clearing the drawing buffer between frames. With the buffer preserved, any
/// cell whose content is erased — a TUI redraw, an `EL`/`ED`, a scroll-region
/// change, a reflow that drops rows — keeps its old glyph pixels on the canvas,
/// producing stale-frame glyph soup in the upper region of a pane that only
/// heals when something forces a full clear ("garbled until scrolled").
///
/// The mobile mount (`mobile_xterm.rs`) runs WebGL without this flag and does
/// not show the symptom; this mount passed `true` and did. `true` was there to
/// let the remount cover read pixels out of the GL canvas, which is now handled
/// by forcing a synchronous full-range repaint immediately before the capture
/// (`use_drop`), so the buffer is read while it is still valid.
pub(crate) fn try_activate_webgl_addon(
    window: &web_sys::Window,
    term_val: &JsValue,
) -> Option<JsValue> {
    let global_val = js_sys::Reflect::get(window, &JsValue::from_str("WebglAddon")).ok()?;
    let ctor_val = if global_val.is_function() {
        global_val
    } else {
        js_sys::Reflect::get(&global_val, &JsValue::from_str("WebglAddon")).ok()?
    };
    if !ctor_val.is_function() {
        return None;
    }
    let ctor: js_sys::Function = ctor_val.dyn_into().ok()?;
    let instance = js_sys::Reflect::construct(&ctor, &js_sys::Array::of1(&JsValue::FALSE)).ok()?;
    let activate_val = js_sys::Reflect::get(&instance, &JsValue::from_str("activate")).ok()?;
    let activate_fn: js_sys::Function = activate_val.dyn_into().ok()?;
    let _ = activate_fn.call1(&instance, term_val);
    Some(instance)
}

/// Wire WebGL context-loss recovery on `webgl_addon`:
///
/// - LOSS → dispose the addon (xterm falls back to the DOM renderer instead
///   of painting nothing) and arm a `webglcontextrestored` listener on the
///   addon's canvas. Closing hidden panes keep their canvases mounted under
///   `display:none`, so WKWebView reclaims GL contexts for occluded panes;
///   the DOM fallback keeps stale glyph spans under WKWebView (see the
///   renderer comment in xterm_mount.rs), so the pane paints scattered
///   glyphs until a remount/resize forces a full repaint — unless we
///   recover WebGL here.
/// - RESTORE → construct a FRESH WebglAddon (a disposed addon cannot be
///   reused), rebuild the glyph atlas, and re-wire this same recovery on
///   the new addon.
///
/// Returns the loss closure; the caller MUST root it (e.g. in the cleanup
/// struct) or the listener dies with the scope.
// ponytail: closures created by later loss/restore cycles are intentionally
// leaked via Closure::forget — a few KB per GPU context reset; rooting them
// would need a mutable holder shared with the cleanup struct.
pub(crate) fn wire_webgl_context_recovery(
    window: &web_sys::Window,
    term_val: &JsValue,
    webgl_addon: &JsValue,
) -> Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>> {
    use wasm_bindgen::JsCast;
    let on_loss_fn: js_sys::Function =
        js_sys::Reflect::get(webgl_addon, &JsValue::from_str("onContextLoss"))
            .ok()?
            .dyn_into()
            .ok()?;
    let addon_for_loss = webgl_addon.clone();
    let term_for_loss = term_val.clone();
    let window_for_loss = window.clone();
    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: JsValue| {
        if let Ok(dispose_fn) =
            js_sys::Reflect::get(&addon_for_loss, &JsValue::from_str("dispose"))
                .and_then(|v| v.dyn_into::<js_sys::Function>())
        {
            web_sys::console::warn_1(
                &"[XtermMount] WebGL context lost; falling back to Canvas renderer until restore"
                    .into(),
            );
            let _ = dispose_fn.call0(&addon_for_loss);
        }
        // Middle tier: canvas renderer keeps a single-canvas paint (no DOM
        // span staleness) while WebGL is gone; if it fails too, xterm's DOM
        // fallback continues.
        let canvas_holder: std::rc::Rc<std::cell::RefCell<Option<JsValue>>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                try_activate_canvas_addon(&window_for_loss, &term_for_loss),
            ));
        if let Some(c) = canvas_holder.borrow().as_ref() {
            if let Some(cl) = wire_canvas_context_loss_logging(c) {
                cl.forget();
            }
        }
        // The canvas keeps receiving webglcontextrestored even after
        // removal from the DOM (spec targets the element, not the tree).
        // The canvas lives on the addon's renderer (`addon._renderer._canvas`
        // in the vendored bundle), not on the WebglAddon itself.
        let Ok(canvas): Result<web_sys::EventTarget, _> =
            js_sys::Reflect::get(&addon_for_loss, &JsValue::from_str("_renderer"))
                .and_then(|r| js_sys::Reflect::get(&r, &JsValue::from_str("_canvas")))
                .and_then(|v| v.dyn_into())
        else {
            return;
        };
        let term_for_restore = term_for_loss.clone();
        let window_for_restore = window_for_loss.clone();
        let restored = wasm_bindgen::closure::Closure::once(move |_: JsValue| {
            // Drop the interim canvas renderer before re-attaching WebGL.
            if let Some(c) = canvas_holder.borrow_mut().take() {
                if let Ok(dispose_fn) =
                    js_sys::Reflect::get(&c, &JsValue::from_str("dispose"))
                        .and_then(|v| v.dyn_into::<js_sys::Function>())
                {
                    let _ = dispose_fn.call0(&c);
                }
            }
            if let Some(new_addon) =
                try_activate_webgl_addon(&window_for_restore, &term_for_restore)
            {
                web_sys::console::log_1(
                    &"[XtermMount] WebGL context restored; re-attached WebglAddon".into(),
                );
                reset_glyph_atlas(&term_for_restore);
                if let Some(next) = wire_webgl_context_recovery(
                    &window_for_restore,
                    &term_for_restore,
                    &new_addon,
                ) {
                    next.forget();
                }
            }
        });
        let opts = web_sys::AddEventListenerOptions::new();
        opts.set_once(true);
        let _ = canvas.add_event_listener_with_callback_and_add_event_listener_options(
            "webglcontextrestored",
            restored.as_ref().unchecked_ref(),
            &opts,
        );
        restored.forget();
    })
        as Box<dyn FnMut(JsValue)>);
    let _ = on_loss_fn.call1(webgl_addon, closure.as_ref().unchecked_ref());
    Some(closure)
}

/// Call `on_change` once per devicePixelRatio change (window moved between
/// monitors, zoom), re-registering matchMedia for the NEW ratio each time.
/// Stops re-arming once `active` goes false (mount dropped), so dead mounts
/// don't leak a listener chain. Closures for live mounts are intentionally
/// leaked (`Closure::forget`) — one small closure per DPR change.
pub(crate) fn watch_dpr_change(on_change: Rc<dyn Fn()>, active: Rc<RefCell<bool>>) {
    fn install(on_change: Rc<dyn Fn()>, active: Rc<RefCell<bool>>) {
        use wasm_bindgen::JsCast;
        if !*active.borrow() {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let dpr = window.device_pixel_ratio();
        let Ok(Some(media)) =
            window.match_media(&format!("(resolution: {}dppx)", dpr))
        else {
            return;
        };
        let rearm = on_change.clone();
        let active_for_rearm = active.clone();
        let closure = wasm_bindgen::closure::Closure::once(move |_: JsValue| {
            rearm();
            install(rearm, active_for_rearm);
        });
        let opts = web_sys::AddEventListenerOptions::new();
        opts.set_once(true);
        let _ = media.add_event_listener_with_callback_and_add_event_listener_options(
            "change",
            closure.as_ref().unchecked_ref(),
            &opts,
        );
        closure.forget();
    }
    install(on_change, active);
}

/// Composite every canvas under `.xterm-screen` into a PNG data URL.
///
/// The composite is sized from the source canvas's own `width/height`
/// (device pixels), not the element's `clientWidth/Height` (CSS pixels):
/// device-px sizing is correct on Retina (canvas backing store is CSS×DPR)
/// and still works when `use_drop` runs after Dioxus has detached the node
/// (`clientWidth` of a detached element is 0). `.xterm-screen` fills the
/// mount container and the WebGL renderer paints into one canvas, so all
/// canvases draw at (0,0) in composite space.
pub(crate) fn capture_screen_data_url(
    container: &web_sys::Element,
) -> Option<(String, (f64, f64))> {
    let canvases = container.query_selector_all(".xterm-screen canvas").ok()?;
    if canvases.length() == 0 {
        return None;
    }
    let document = web_sys::window()?.document()?;
    let composite: web_sys::HtmlCanvasElement =
        document.create_element("canvas").ok()?.dyn_into().ok()?;
    let mut drew_any = false;
    let mut css_size = (0.0, 0.0);
    for i in 0..canvases.length() {
        let Some(node) = canvases.get(i) else {
            continue;
        };
        let Ok(source) = node.dyn_into::<web_sys::HtmlCanvasElement>() else {
            continue;
        };
        if source.width() == 0 || source.height() == 0 {
            continue;
        }
        if composite.width() == 0 {
            composite.set_width(source.width());
            composite.set_height(source.height());
            // The bitmap is device-px; the cover is sized in CSS px. The
            // renderer sets the canvas' inline style to its CSS size — read
            // it here, while the pane is (possibly detached) and its
            // clientWidth may already be 0.
            let parse_px = |prop: &str| {
                source
                    .style()
                    .get_property_value(prop)
                    .ok()
                    .and_then(|v| v.trim_end_matches("px").parse::<f64>().ok())
                    .unwrap_or(0.0)
            };
            css_size = (parse_px("width"), parse_px("height"));
        }
        let ctx: web_sys::CanvasRenderingContext2d =
            composite.get_context("2d").ok()??.dyn_into().ok()?;
        if ctx
            .draw_image_with_html_canvas_element(&source, 0.0, 0.0)
            .is_ok()
        {
            drew_any = true;
        }
    }
    if !drew_any {
        return None;
    }
    // A cover larger than this is storing scrollback-scale PNGs for nothing;
    // the pane repaints once real output arrives regardless.
    const MAX_COVER_DATA_URL_BYTES: usize = 2 * 1024 * 1024;
    let url = composite.to_data_url().ok()?;
    if url.len() > MAX_COVER_DATA_URL_BYTES || url == "data:," {
        return None;
    }
    Some((url, css_size))
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
    let Some(payload) = filter_graphics_for_term(term_val, bytes) else {
        return;
    };
    write_payload_to_term(term_val, payload.as_ref());
}

/// Attach the vendored Kitty graphics addon: strips APC G image sequences
/// from the PTY stream, paints overlay canvases, answers protocol queries.
/// The returned `feed` fn is rooted as a `__athenaKittyFeed` expando on the
/// Terminal so write paths pick it up without threading a handle through
/// every callsite. Returns `None` when the defer-loaded addon script has not
/// evaluated yet — callers should retry instead of writing unfiltered.
pub(crate) fn attach_kitty_graphics(term_val: &JsValue, respond_js: &JsValue) -> Option<JsValue> {
    let window = web_sys::window()?;
    let attached = js_sys::Reflect::get(&window, &JsValue::from_str("AthenaKitty"))
        .ok()
        .filter(|v| !v.is_undefined())
        .and_then(|kitty| {
            js_sys::Reflect::get(&kitty, &JsValue::from_str("attach"))
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        })
        .and_then(|attach_fn| attach_fn.call2(&term_val, &term_val, respond_js).ok());
    if let Some(addon) = attached {
        if let Ok(feed_fn) = js_sys::Reflect::get(&addon, &JsValue::from_str("feed")) {
            let _ = js_sys::Reflect::set(
                &term_val,
                &JsValue::from_str("__athenaKittyFeed"),
                &feed_fn,
            );
        }
        return js_sys::Reflect::set(&term_val, &JsValue::from_str("__athenaKitty"), &addon)
            .ok()
            .map(|_| addon);
    }
    None
}

/// Run a chunk through the Kitty graphics filter (expando set by
/// `attach_kitty_graphics` in `xterm_mount` when the vendored addon loaded).
/// The filter strips APC G image sequences — rendering them as overlay
/// canvases itself — and returns the remaining bytes, possibly spliced with
/// cursor-advance newlines. `None` means the chunk was entirely image data.
pub(crate) fn filter_graphics_for_term(term_val: &JsValue, bytes: &[u8]) -> Option<JsValue> {
    let bytes_arr = js_sys::Uint8Array::from(bytes);
    let filter = js_sys::Reflect::get(term_val, &JsValue::from_str("__athenaKittyFeed"))
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
    match filter {
        Some(f) => {
            let out = f.call1(term_val, bytes_arr.as_ref()).ok()?;
            if out.is_null() || out.is_undefined() {
                None
            } else {
                Some(out)
            }
        }
        None => {
            // ponytail: no filter attached (addon script not yet loaded).
            // Stock xterm drops well-formed APC, but a payload split across
            // chunks can still leak glyphs — mount code must attach before
            // the first PTY write.
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                web_sys::console::warn_1(&JsValue::from_str(
                    "[athena] kitty graphics filter missing; writing raw chunk",
                ));
            }
            Some(bytes_arr.into())
        }
    }
}

/// Remove Kitty unicode-placeholder runs (U+10EEEE plus trailing combining
/// diacritics U+0300–U+036F) from text destined for the terminal. Replayed
/// snapshots captured while an image was displayed contain these runes and
/// would otherwise repaint as a missing-glyph grid.
pub(crate) fn strip_kitty_placeholders(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\u{10EEEE}') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut skip = false;
    for ch in text.chars() {
        if ch == '\u{10EEEE}' {
            skip = true;
            continue;
        }
        if skip && ('\u{0300}'..='\u{036F}').contains(&ch) {
            continue;
        }
        skip = false;
        out.push(ch);
    }
    std::borrow::Cow::Owned(out)
}

pub(crate) fn write_str_to_term(term_val: &JsValue, text: &str) {
    let text = strip_kitty_placeholders(text);
    // Route through the Kitty filter as well: snapshot replay may carry APC
    // graphics frames; writing them raw would leak payload bytes as text or,
    // with the filter attached, paint overlays twice.
    if let Some(payload) = filter_graphics_for_term(term_val, text.as_bytes()) {
        write_payload_to_term(term_val, payload.as_ref());
    }
}

fn write_payload_to_term(term_val: &JsValue, payload: &JsValue) {
    let Ok(write_val) = js_sys::Reflect::get(term_val, &JsValue::from_str("write")) else {
        return;
    };
    let Ok(write_fn) = write_val.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = write_fn.call1(term_val, payload);
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

// FitAddon recomputes xterm's rows/columns AND synchronously clears the WebGL
// canvas + reflows the buffer (addon-fit: renderService.clear() then
// terminal.resize(), one statement). The repaint that follows is queued through
// xterm's render debouncer, so a starved or dropped frame can leave the
// cleared/stale canvas on screen until something else forces a paint (the
// "garbled until scrolled" report: scroll fixes it because scrolling forces a
// synchronous repaint). So after fitting we synchronously render the visible
// rows in the SAME JS task, guaranteeing the compositor only ever presents a
// complete frame. The deferred rAF repaint in schedule_fit remains as a second
// pass (CanvasAddon historically needed a frame to settle its canvas size).
pub(crate) fn call_fit(fit_instance: &JsValue, container: &web_sys::Element, term_val: &JsValue) {
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
    render_visible_rows_sync(term_val);
}

/// Synchronously paint the full visible row range, bypassing the debounced
/// refresh queue. No-op if the renderer internals are unavailable.
pub(crate) fn render_visible_rows_sync(term_val: &JsValue) {
    let rows = js_sys::Reflect::get(term_val, &JsValue::from_str("rows"))
        .ok()
        .and_then(|v| v.as_f64())
        .map(|v| v as i32)
        .unwrap_or(24);
    if rows <= 0 {
        return;
    }
    let Ok(core) = js_sys::Reflect::get(term_val, &JsValue::from_str("_core")) else {
        return;
    };
    let Ok(service) = js_sys::Reflect::get(&core, &JsValue::from_str("_renderService")) else {
        return;
    };
    // RenderService gates handleResize on its visibility IntersectionObserver:
    // while _isPaused the renderer resize is DEFERRED (handleResize only
    // re-arms _pausedResizeTask), so a sync paint would draw with a stale
    // render model. Skip; the pane is hidden and the unpause path does a full
    // refresh.
    let paused = js_sys::Reflect::get(&service, &JsValue::from_str("_isPaused"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if paused {
        return;
    }
    let _ = call0_2(
        &service,
        "_renderRows",
        &JsValue::from_f64(0.0),
        &JsValue::from_f64((rows - 1) as f64),
    );
}

fn call0_2(target: &JsValue, name: &str, a: &JsValue, b: &JsValue) -> Result<JsValue, JsValue> {
    let f = js_sys::Reflect::get(target, &JsValue::from_str(name))?;
    let f: js_sys::Function = f.dyn_into()?;
    f.call2(target, a, b)
}

// Each argument is an independent piece of fit state owned by the caller;
// grouping them would churn all 8 call sites for zero behavioral gain.
#[allow(clippy::too_many_arguments)]
pub(crate) fn schedule_fit(
    window: &web_sys::Window,
    fit_instance: &JsValue,
    container: &web_sys::Element,
    term_val: &JsValue,
    pending: &Rc<RefCell<bool>>,
    dirty: &Rc<Cell<bool>>,
    active: &Rc<RefCell<bool>>,
    viewport: &Rc<RefCell<Option<ViewportState>>>,
    follow_until: &Rc<Cell<f64>>,
    last_bottom_at: &Rc<Cell<f64>>,
) {
    // ResizeObserver, font updates, visibility restoration, and pane swaps can
    // all request a fit in the same frame. Keep the whole fit/repaint pair in
    // flight so xterm does not resize/repaint repeatedly while WebKit is still
    // settling flex layout. Requests that arrive while a pair is in flight are
    // NOT dropped: they mark the fit dirty so a final fit runs at the settled
    // layout size. Dropping them let bursts from pane close + weight
    // normalization leave the pane stuck with intermediate geometry (the
    // "fonts suddenly huge after closing a pane" bug).
    if !*active.borrow() {
        return;
    }
    if *pending.borrow() {
        dirty.set(true);
        return;
    }
    let captured = capture_viewport(term_val);
    // The one-shot restore below runs before the deferred TUI redraw burst
    // (SIGWINCH → reprint) that the resize itself triggers. Record follow
    // intent so the write path keeps re-pinning a bottom-following pane until
    // the burst settles — otherwise the pane can stick scrolled up. Schedule
    // delayed re-pins as well: an idle pane writes nothing after the burst,
    // so only the timed re-pins can catch the final buffer state.
    if note_fit_viewport_intent(captured.at_bottom, follow_until, last_bottom_at) {
        schedule_follow_repins(term_val, active, follow_until);
    }
    *viewport.borrow_mut() = Some(captured);
    *pending.borrow_mut() = true;

    let fit_for_raf = fit_instance.clone();
    let container_for_raf = container.clone();
    let term_for_fit = term_val.clone();
    let term_for_refresh = term_val.clone();
    let pending_for_raf = pending.clone();
    let dirty_for_raf = dirty.clone();
    let active_for_raf = active.clone();
    let viewport_for_raf = viewport.clone();
    let window_for_refresh = window.clone();
    let fit_for_refresh = fit_instance.clone();
    let container_for_refresh = container.clone();
    let follow_for_refresh = follow_until.clone();
    let last_bottom_for_refresh = last_bottom_at.clone();
    // The first RAF commits the new xterm grid dimensions. The second RAF
    // lets xterm commit its resized buffer before repainting and restoring the
    // viewport intent. This also gives full-screen applications one frame to
    // process the PTY's SIGWINCH redraw.
    let raf_closure = wasm_bindgen::closure::Closure::once_into_js(move || {
        if !*active_for_raf.borrow() {
            *pending_for_raf.borrow_mut() = false;
            dirty_for_raf.set(false);
            *viewport_for_raf.borrow_mut() = None;
            unfreeze_screen_now(&container_for_raf);
            return;
        }
        call_fit(&fit_for_raf, &container_for_raf, &term_for_fit);

        let pending_for_refresh = pending_for_raf.clone();
        let dirty_for_refresh = dirty_for_raf.clone();
        let dirty_for_refresh_fallback = dirty_for_raf.clone();
        let active_for_refresh = active_for_raf.clone();
        let viewport_for_refresh = viewport_for_raf.clone();
        let pending_for_refresh_fallback = pending_for_refresh.clone();
        let active_for_refresh_fallback = active_for_refresh.clone();
        let viewport_for_refresh_fallback = viewport_for_refresh.clone();
        let term_for_refresh_fallback = term_for_refresh.clone();
        let window_for_fallback = window_for_refresh.clone();
        let fit_for_fallback = fit_for_refresh.clone();
        let container_for_fallback = container_for_refresh.clone();
        let follow_for_fallback = follow_for_refresh.clone();
        let last_bottom_for_fallback = last_bottom_for_refresh.clone();
        let refresh_window = window_for_refresh.clone();
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
            // A coalesced RO tick landed while this pair was in flight: the
            // layout may have settled further, so run one more pair at the
            // final geometry. Only reveal the frozen screen when no further
            // pair follows — the follow-up pair reveals at its own tail.
            if dirty_for_refresh.replace(false) && *active_for_refresh.borrow() {
                schedule_fit(
                    &refresh_window,
                    &fit_for_refresh,
                    &container_for_refresh,
                    &term_for_refresh,
                    &pending_for_refresh,
                    &dirty_for_refresh,
                    &active_for_refresh,
                    &viewport_for_refresh,
                    &follow_for_refresh,
                    &last_bottom_for_refresh,
                );
            } else {
                unfreeze_screen_delayed(&refresh_window, &container_for_refresh);
            }
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
            unfreeze_screen_now(&container_for_fallback);
            if dirty_for_refresh_fallback.replace(false)
                && *active_for_refresh_fallback.borrow()
            {
                schedule_fit(
                    &window_for_fallback,
                    &fit_for_fallback,
                    &container_for_fallback,
                    &term_for_refresh_fallback,
                    &pending_for_refresh_fallback,
                    &dirty_for_refresh_fallback,
                    &active_for_refresh_fallback,
                    &viewport_for_refresh_fallback,
                    &follow_for_fallback,
                    &last_bottom_for_fallback,
                );
            }
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
        unfreeze_screen_now(container);
        if dirty.replace(false) && *active.borrow() {
            schedule_fit(
                window,
                fit_instance,
                container,
                term_val,
                pending,
                dirty,
                active,
                viewport,
                follow_until,
                last_bottom_at,
            );
        }
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
// WKWebView accelerated-canvas quirk: when a pane close/open doubles a flex
// share, the container's CSS box grows instantly while the glyph canvas still
// holds its old bitmap. The compositor can present that stale bitmap scaled
// to the grown box for the ~100ms between the ResizeObserver tick and the
// debounced fit pair's first post-fit repaint — the "giant blurry glyphs"
// flash. Hiding .xterm-screen for the refit window shows an empty pane
// instead; the reveal runs one extra rAF after the fit pair's repaint so a
// real frame at the new geometry has been composited, not just queued.
// ---------------------------------------------------------------------------
pub(crate) fn freeze_screen(container: &web_sys::Element) {
    set_screen_opacity(container, "0");
}

fn set_screen_opacity(container: &web_sys::Element, value: &str) {
    let Ok(screens) = container.query_selector_all(".xterm-screen") else {
        return;
    };
    for i in 0..screens.length() {
        if let Some(el) = screens
            .item(i)
            .and_then(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = el.style().set_property("opacity", value);
        }
    }
}

/// Reveal the screen on the NEXT animation frame. The caller has just run the
/// fit pair's final repaint; one more rAF lets that paint reach the compositor
/// before the canvas becomes visible again.
pub(crate) fn unfreeze_screen_delayed(window: &web_sys::Window, container: &web_sys::Element) {
    let inner = container.clone();
    let reveal = wasm_bindgen::closure::Closure::once_into_js(move || {
        set_screen_opacity(&inner, "");
    });
    if window
        .request_animation_frame(reveal.as_ref().unchecked_ref())
        .is_err()
    {
        // rAF unavailable (headless/torn-down window): reveal immediately
        // rather than strand a hidden canvas.
        set_screen_opacity(container, "");
    }
}

/// Fallback reveal for schedule_fit's error paths (rAF request rejected).
pub(crate) fn unfreeze_screen_now(container: &web_sys::Element) {
    set_screen_opacity(container, "");
}

// ---------------------------------------------------------------------------
// Rebuild the renderer's glyph atlas from the CURRENT font metrics and repaint
// the full viewport before returning — in this same JS task.
//
// Why the repaint is part of the reset: a cell's glyph coordinates are recorded
// in the render model when the row is painted, and they index into the atlas
// that existed at that moment. xterm installs a fresh atlas whenever the cell
// metrics change (a webfont finishing its load, a font/size option change, a
// DPR change) and the rows already in the model are NOT re-derived — they keep
// drawing their old slot numbers against the new atlas, i.e. correct cells in
// the correct font showing the wrong characters. Symptom: scrambled glyphs at
// the top of the pane (everything painted before the swap) and correct text
// further down (rows rebuilt against the new atlas).
//
// Clearing the texture atlas alone does not close the window: it resets the
// atlas pages and the model, then relies on a *debounced* viewport redraw that
// a busy or hidden pane can starve. So reset, then force the repaint here —
// the public `refresh()` for the paused/hidden path (it arms RenderService's
// `_needsFullRefresh`, replayed as a full refresh on unpause) and a synchronous
// paint of the visible range for the visible one. Both run before the caller
// can write the next byte.
//
// Also covers the remount case the old code handled: the atlas may have been
// built before the webfont's real advance widths were in place, and xterm only
// re-measures on an options.font* CHANGE — a value-preserving refit leaves
// oversized cells (the "font suddenly 2x" symptom) until restarted.
// ---------------------------------------------------------------------------
pub(crate) fn reset_glyph_atlas(term_val: &JsValue) {
    // Primary: public Terminal.clearTextureAtlas() (present in the vendored
    // bundle). Unconditionally rebuilds the WebGL glyph atlas and triggers a
    // full repaint — this is the state that holds oversized rasterization, and
    // a value-preserving re-measure alone would NOT rebuild it (CharSizeService
    // only emits onCharSizeChange when metrics actually differ).
    let _ = call0(term_val, "clearTextureAtlas");
    // Secondary: round-trip options.fontSize/fontFamily (runs the renderer's
    // options-changed path even when the value is unchanged).
    if let Ok(options) = js_sys::Reflect::get(term_val, &JsValue::from_str("options")) {
        if let Ok(size) = js_sys::Reflect::get(&options, &JsValue::from_str("fontSize")) {
            let _ = js_sys::Reflect::set(&options, &JsValue::from_str("fontSize"), &size);
        }
        if let Ok(family) = js_sys::Reflect::get(&options, &JsValue::from_str("fontFamily")) {
            let _ = js_sys::Reflect::set(&options, &JsValue::from_str("fontFamily"), &family);
        }
    }
    // Secondary: direct CharSizeService measure (name varies by xterm version).
    if let Ok(core) = js_sys::Reflect::get(term_val, &JsValue::from_str("_core")) {
        for service_name in ["_charSizeService", "_charSizeServiceProxy"] {
            if let Ok(service) = js_sys::Reflect::get(&core, &JsValue::from_str(service_name)) {
                if !service.is_undefined() {
                    let _ = call0(&service, "measure");
                }
            }
        }
    }
    // Repaint in this task. Terminal.clearTextureAtlas() already queues a
    // full refresh of its own, but that goes through the render debouncer and
    // is exactly the frame a busy or hidden pane can starve. So: the public
    // refresh() (also arms `_needsFullRefresh` for the paused/hidden path,
    // replayed as a full refresh on unpause) plus a synchronous paint of the
    // visible rows, before the caller can write the next byte.
    refresh_full(term_val);
    render_visible_rows_sync(term_val);
}

fn call0(target: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    let f = js_sys::Reflect::get(target, &JsValue::from_str(name))?;
    let f: js_sys::Function = f.dyn_into()?;
    f.call0(target)
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

    // Load each family in the stack individually. FontFaceSet.load() with a
    // family *list* only guarantees the first available family gets requested
    // in some WebKit builds; if the primary family is already cached the Nerd
    // Font fallback face may never be requested, leaving glyph fallback (icon
    // PUA codepoints) broken for the whole session. Per-family loads also
    // cover the bold face used for bright/ANSI-bold terminal text.
    if let Ok(load_val) = js_sys::Reflect::get(&fonts, &JsValue::from_str("load")) {
        if let Ok(load_fn) = load_val.dyn_into::<js_sys::Function>() {
            let size = font_size.max(1.0);
            let specs: Vec<String> = font_family
                .split(',')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .flat_map(|f| [format!("{size}px {f}"), format!("bold {size}px {f}")])
                .collect();
            for spec in specs {
                if let Ok(promise) = load_fn.call1(&fonts, &JsValue::from_str(&spec)) {
                    // Rejections are fine: a family name may be a generic
                    // keyword or an uninstalled font; fallback still works.
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

// Resume capture lives in xterm_mount.rs via ResumeScanner (re-exported at
// crate::utils::resume_scanner); keep the parsing rules in the shared crate.
