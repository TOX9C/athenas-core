//! Stash of per-pane `XtermCleanup`s across transient Dioxus unmounts.
//!
//! A pane swap or space switch unmounts `XtermMount`, but the PTY session
//! and its xterm instance are still live. Parking moves the full `XtermCleanup`
//! — Terminal JsValue, unlisten handle, and every rooted closure (onData,
//! onResize, kitty, WebGL context-loss, focus recovery) — into this registry
//! so the JS GC cannot reap the listeners while the component is gone.
//! The `.xterm` element is moved offscreen (not display:none: WKWebView can
//! discard a hidden canvas' GL backing). The next mount takes the cleanup
//! back and reattaches the element; teardown still runs on a real pane close.

use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

use super::xterm_mount::XtermCleanup;

thread_local! {
    static INSTANCES: RefCell<HashMap<String, XtermCleanup>> =
        RefCell::new(HashMap::new());
}

/// Real close path never calls this: any stash implies a future remount.
/// `take` is consumption — after a mount reattaches, the cleanup lives
/// in that mount's Dioxus scope again.
pub(crate) fn take(pane_id: &str) -> Option<XtermCleanup> {
    INSTANCES.with(|i| i.borrow_mut().remove(pane_id))
}

pub(crate) fn stash(pane_id: String, cleanup: XtermCleanup) {
    INSTANCES.with(|i| i.borrow_mut().insert(pane_id, cleanup));
}

/// Offscreen holder for parked terminal elements. Lazily created.
pub(crate) fn parking_lot() -> Option<web_sys::HtmlElement> {
    let document = web_sys::window()?.document()?;
    if let Some(el) = document.get_element_by_id("athena-xterm-parking") {
        return el.dyn_into::<web_sys::HtmlElement>().ok();
    }
    let el: web_sys::HtmlElement = document
        .create_element("div")
        .ok()?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()?;
    el.set_id("athena-xterm-parking");
    let _ = el.set_attribute(
        "style",
        "position: fixed; left: -10000px; top: -10000px; width: 80px; \
         height: 24px; visibility: hidden; pointer-events: none;",
    );
    let _ = document.body()?.append_child(&el);
    Some(el)
}
