//! File/image drop handling for PTY panes.
//!
//! Finder-like drops arrive through Tauri's native drag events with paths.
//! Screenshot tools such as CleanShot X can instead provide an image promise
//! to the WebView without a path; the DOM fallback reads that image and stages
//! it through the backend before inserting the staged path.

use super::terminal_input::{format_dropped_paths, TerminalInputRouter};
use crate::stores::terminal::use_terminal_registry;
use crate::tauri_bridge;
use base64::Engine;
use dioxus::prelude::*;
use gloo::timers::future::TimeoutFuture;
use js_sys::{Function, Reflect, Uint8Array};
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const NATIVE_DROP_EVENT: &str = "tauri://drag-drop";
const NATIVE_DRAG_ENTER_EVENT: &str = "tauri://drag-enter";
const NATIVE_DRAG_OVER_EVENT: &str = "tauri://drag-over";
const NATIVE_DRAG_LEAVE_EVENT: &str = "tauri://drag-leave";
const MAX_IMAGE_BYTES: f64 = 20.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Copy, Deserialize)]
struct DropPosition {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct NativeDropPayload {
    #[serde(default)]
    paths: Vec<String>,
    position: Option<DropPosition>,
}

#[derive(Clone, Copy)]
struct DropMarker {
    x: f64,
    y: f64,
    at_ms: f64,
}

fn css_position(position: DropPosition) -> (f64, f64) {
    // The native drag position arrives already in the WebView's logical (CSS)
    // pixel space on macOS. wry derives it from
    // `NSDraggingInfo.draggingLocation()` and the WKWebView frame — both in
    // logical points — and Tauri forwards that value wrapped in a
    // `PhysicalPosition` *without* any DPI conversion. Dividing by
    // `devicePixelRatio` here halves both axes on Retina (2x), which shifts the
    // hit-test left: a drop over the right pane of a side-by-side grid resolves
    // to the left pane. Pass the coordinate through unchanged so it matches the
    // CSS-pixel space `document.elementFromPoint` expects (and the DOM
    // `dragover`/`drop` `clientX`/`clientY`, which are also CSS px).
    (position.x, position.y)
}

fn event_css_position(event: &JsValue) -> Option<(f64, f64)> {
    let x = Reflect::get(event, &JsValue::from_str("clientX"))
        .ok()?
        .as_f64()?;
    let y = Reflect::get(event, &JsValue::from_str("clientY"))
        .ok()?
        .as_f64()?;
    Some((x, y))
}

fn pane_at_point(x: f64, y: f64) -> Option<String> {
    let document = web_sys::window()?.document()?;
    let mut element = document.element_from_point(x as f32, y as f32);
    while let Some(current) = element {
        if let Some(pane_id) = current.get_attribute("data-pane-id") {
            if document
                .get_element_by_id(&pane_id)
                .and_then(|el| el.get_attribute("data-terminal-renderer"))
                .is_some()
            {
                return Some(pane_id);
            }
        }
        element = current.parent_element();
    }
    None
}

fn set_file_drop_target(target: Option<&str>) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Ok(nodes) = document.query_selector_all("[data-pane-id]") else {
        return;
    };
    for index in 0..nodes.length() {
        let Some(node) = nodes.item(index) else {
            continue;
        };
        let Ok(element) = node.dyn_into::<web_sys::Element>() else {
            continue;
        };
        let is_target = target
            .zip(element.get_attribute("data-pane-id"))
            .is_some_and(|(target, pane_id)| target == pane_id);
        let mut classes = element
            .get_attribute("class")
            .unwrap_or_default()
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        classes.retain(|class| class != "is-file-drop-target");
        if is_target {
            classes.push("is-file-drop-target".to_string());
        }
        let _ = element.set_attribute("class", &classes.join(" "));
    }
}

fn focus_pane(pane_id: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(mount) = document.get_element_by_id(pane_id) else {
        return;
    };
    if let Ok(Some(textarea)) = mount.query_selector(".xterm-helper-textarea") {
        if let Ok(textarea) = textarea.dyn_into::<web_sys::HtmlElement>() {
            let _ = textarea.focus();
        }
    }
}

fn mark_native_drop(marker: &Rc<RefCell<Option<DropMarker>>>, x: f64, y: f64) {
    *marker.borrow_mut() = Some(DropMarker {
        x,
        y,
        at_ms: js_sys::Date::now(),
    });
}

fn recent_native_drop(marker: &Rc<RefCell<Option<DropMarker>>>, x: f64, y: f64) -> bool {
    marker.borrow().is_some_and(|marker| {
        js_sys::Date::now() - marker.at_ms < 750.0
            && (marker.x - x).abs() < 24.0
            && (marker.y - y).abs() < 24.0
    })
}

fn file_from_drop_event(event: &JsValue) -> Option<JsValue> {
    let data_transfer = Reflect::get(event, &JsValue::from_str("dataTransfer")).ok()?;
    let files = Reflect::get(&data_transfer, &JsValue::from_str("files")).ok()?;
    let length = Reflect::get(&files, &JsValue::from_str("length"))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0) as u32;
    if length > 0 {
        if let Ok(item) = Reflect::get(&files, &JsValue::from_str("item"))
            .and_then(|value| value.dyn_into::<Function>().map_err(|_| JsValue::NULL))
        {
            if let Ok(file) = item.call1(&files, &JsValue::from_f64(0.0)) {
                return Some(file);
            }
        }
    }

    // WebKit and file-promise drag sources can expose a DataTransferItem
    // before they populate DataTransfer.files. CleanShot X commonly follows
    // this route when dragging a floating screenshot directly from its UI.
    let items = Reflect::get(&data_transfer, &JsValue::from_str("items")).ok()?;
    let item_count = Reflect::get(&items, &JsValue::from_str("length"))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0) as u32;
    let item = Reflect::get(&items, &JsValue::from_str("item"))
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    for index in 0..item_count {
        let data_item = item.call1(&items, &JsValue::from_f64(index as f64)).ok()?;
        let kind = file_string_property(&data_item, "kind");
        if kind != "file" {
            continue;
        }
        let get_as_file = Reflect::get(&data_item, &JsValue::from_str("getAsFile"))
            .ok()?
            .dyn_into::<Function>()
            .ok()?;
        if let Ok(file) = get_as_file.call0(&data_item) {
            if !file.is_null() && !file.is_undefined() {
                return Some(file);
            }
        }
    }
    None
}

fn file_string_property(file: &JsValue, property: &str) -> String {
    Reflect::get(file, &JsValue::from_str(property))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

async fn read_file_bytes(file: JsValue) -> Result<Vec<u8>, String> {
    let array_buffer = Reflect::get(&file, &JsValue::from_str("arrayBuffer"))
        .map_err(|_| "image data is unavailable")?
        .dyn_into::<Function>()
        .map_err(|_| "image data is unavailable")?
        .call0(&file)
        .map_err(|_| "image data is unavailable")?;
    let array_buffer = JsFuture::from(
        array_buffer
            .dyn_into::<js_sys::Promise>()
            .map_err(|_| "image data is unavailable")?,
    )
    .await
    .map_err(|_| "image data could not be read")?;
    Ok(Uint8Array::new(&array_buffer).to_vec())
}

fn bytes_to_base64(bytes: &[u8]) -> Result<String, String> {
    // Use the same base64 implementation already present in the workspace;
    // keeping encoding in Rust avoids constructing a giant binary JS string.
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn looks_like_image(name: &str, mime: &str) -> bool {
    mime.to_ascii_lowercase().starts_with("image/")
        || ["png", "jpg", "jpeg", "gif", "webp", "heic", "tif", "tiff"]
            .iter()
            .any(|extension| {
                name.to_ascii_lowercase()
                    .ends_with(&format!(".{extension}"))
            })
}

async fn stage_and_insert_image(
    file: JsValue,
    x: f64,
    y: f64,
    router: TerminalInputRouter,
    marker: Rc<RefCell<Option<DropMarker>>>,
) {
    if recent_native_drop(&marker, x, y) {
        return;
    }
    let Some(pane_id) = pane_at_point(x, y) else {
        return;
    };
    let name = file_string_property(&file, "name");
    let mime = file_string_property(&file, "type");
    let size = Reflect::get(&file, &JsValue::from_str("size"))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    if size > MAX_IMAGE_BYTES || !looks_like_image(&name, &mime) {
        web_sys::console::warn_1(
            &"TerminalDrop: only image drops up to 20 MB are supported without a native path"
                .into(),
        );
        return;
    }
    let bytes = match read_file_bytes(file).await {
        Ok(bytes) if bytes.len() as f64 <= MAX_IMAGE_BYTES => bytes,
        _ => {
            web_sys::console::warn_1(&"TerminalDrop: failed to read dropped image".into());
            return;
        }
    };
    let encoded = match bytes_to_base64(&bytes) {
        Ok(encoded) => encoded,
        Err(error) => {
            web_sys::console::warn_1(&format!("TerminalDrop: {error}").into());
            return;
        }
    };
    match tauri_bridge::pty_stage_drop_file(&name, &mime, &encoded).await {
        Ok(path) => {
            if let Some(input) = format_dropped_paths(&[path]) {
                router.enqueue(&pane_id, input);
                focus_pane(&pane_id);
            }
        }
        Err(error) => web_sys::console::warn_1(
            &format!("TerminalDrop: staging image failed: {error:?}").into(),
        ),
    }
}

#[component]
pub fn TerminalDropController() -> Element {
    let terminal_registry = use_terminal_registry();
    let router = terminal_registry.input_router();
    let marker: Rc<RefCell<Option<DropMarker>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let dom_listeners: Rc<RefCell<Vec<(String, wasm_bindgen::JsValue)>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));
    let mut mounted = use_signal(|| false);
    let unlisteners_for_effect = unlisteners.clone();
    let dom_listeners_for_effect = dom_listeners.clone();

    use_effect(move || {
        if mounted() {
            return;
        }

        let register_native = |event_name: &'static str, handler: Box<dyn FnMut(String)>| {
            if let Ok(unlisten) = tauri_bridge::listen(event_name, handler) {
                unlisteners_for_effect.borrow_mut().push(unlisten);
            }
        };

        let marker_for_drop = marker.clone();
        let router_for_drop = router.clone();
        register_native(
            NATIVE_DROP_EVENT,
            Box::new(move |payload| {
                let Ok(payload) = serde_json::from_str::<NativeDropPayload>(&payload) else {
                    set_file_drop_target(None);
                    return;
                };
                let Some(position) = payload.position.map(css_position) else {
                    set_file_drop_target(None);
                    return;
                };
                // Only suppress the DOM fallback when the native event
                // actually carried paths. A data-only CleanShot drop may
                // still emit tauri://drag-drop with an empty path list.
                if !payload.paths.is_empty() {
                    mark_native_drop(&marker_for_drop, position.0, position.1);
                }
                set_file_drop_target(None);
                let Some(pane_id) = pane_at_point(position.0, position.1) else {
                    return;
                };
                if let Some(input) = format_dropped_paths(&payload.paths) {
                    router_for_drop.enqueue(&pane_id, input);
                    focus_pane(&pane_id);
                }
            }),
        );

        register_native(
            NATIVE_DRAG_ENTER_EVENT,
            Box::new(move |payload| {
                if let Ok(payload) = serde_json::from_str::<NativeDropPayload>(&payload) {
                    if let Some(position) = payload.position.map(css_position) {
                        set_file_drop_target(pane_at_point(position.0, position.1).as_deref());
                    }
                }
            }),
        );
        register_native(
            NATIVE_DRAG_OVER_EVENT,
            Box::new(move |payload| {
                if let Ok(payload) = serde_json::from_str::<NativeDropPayload>(&payload) {
                    if let Some(position) = payload.position.map(css_position) {
                        set_file_drop_target(pane_at_point(position.0, position.1).as_deref());
                    }
                }
            }),
        );
        register_native(
            NATIVE_DRAG_LEAVE_EVENT,
            Box::new(move |_payload| set_file_drop_target(None)),
        );

        let Some(window) = web_sys::window() else {
            return;
        };
        let dom_marker = marker.clone();
        let dom_router = router.clone();
        let dragover =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::Event| {
                event.prevent_default();
                if let Some(position) = event_css_position(event.as_ref()) {
                    set_file_drop_target(pane_at_point(position.0, position.1).as_deref());
                }
            }) as Box<dyn FnMut(web_sys::Event)>);
        let drop = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::Event| {
            event.prevent_default();
            event.stop_propagation();
            let Some((x, y)) = event_css_position(event.as_ref()) else {
                set_file_drop_target(None);
                return;
            };
            let Some(file) = file_from_drop_event(event.as_ref()) else {
                set_file_drop_target(None);
                return;
            };
            set_file_drop_target(None);
            let marker = dom_marker.clone();
            let router = dom_router.clone();
            wasm_bindgen_futures::spawn_local(async move {
                TimeoutFuture::new(80).await;
                if recent_native_drop(&marker, x, y) {
                    return;
                }
                stage_and_insert_image(file, x, y, router, marker).await;
            });
        })
            as Box<dyn FnMut(web_sys::Event)>);
        let dragover_js = dragover.into_js_value();
        let drop_js = drop.into_js_value();
        let _ = window.add_event_listener_with_callback_and_bool(
            "dragover",
            dragover_js.unchecked_ref(),
            true,
        );
        let _ =
            window.add_event_listener_with_callback_and_bool("drop", drop_js.unchecked_ref(), true);
        dom_listeners_for_effect
            .borrow_mut()
            .push(("dragover".to_string(), dragover_js));
        dom_listeners_for_effect
            .borrow_mut()
            .push(("drop".to_string(), drop_js));
        mounted.set(true);
    });

    let unlisteners_for_drop = unlisteners.clone();
    let dom_listeners_for_drop = dom_listeners.clone();
    use_drop(move || {
        for unlisten in unlisteners_for_drop.borrow_mut().drain(..) {
            unlisten();
        }
        if let Some(window) = web_sys::window() {
            for (event_name, callback) in dom_listeners_for_drop.borrow_mut().drain(..) {
                let _ = window.remove_event_listener_with_callback_and_bool(
                    &event_name,
                    callback.unchecked_ref(),
                    true,
                );
            }
        }
        set_file_drop_target(None);
    });

    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::looks_like_image;
    #[cfg(target_arch = "wasm32")]
    use super::{css_position, DropPosition};

    #[test]
    fn image_detection_accepts_clean_shot_extensions_and_mime_types() {
        assert!(looks_like_image("Screenshot.png", ""));
        assert!(looks_like_image("capture", "image/png"));
        assert!(!looks_like_image("notes.txt", "text/plain"));
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn native_position_is_already_logical_and_not_rescaled() {
        // Retina-like coordinate: must pass through unchanged, not be halved by
        // `devicePixelRatio`. A halving here is what routed drops over the right
        // pane of a side-by-side grid to the left pane.
        let position = css_position(DropPosition { x: 200.0, y: 100.0 });
        assert_eq!(position, (200.0, 100.0));
    }
}
