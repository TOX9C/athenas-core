use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

/// A single context-menu entry.
#[derive(Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub danger: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            danger: false,
        }
    }
    pub fn danger(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            danger: true,
        }
    }
}

/// Right-click context menu. Wraps a trigger; on contextmenu it opens a themed
/// menu at the cursor and reports the chosen item index via `on_select`.
#[derive(Props, Clone, PartialEq)]
pub struct ContextMenuProps {
    pub items: Vec<MenuItem>,
    pub on_select: EventHandler<usize>,
    pub children: Element,
}

/// Clamp a menu origin so the menu stays fully inside the viewport.
fn clamp_to_viewport(x: i32, y: i32, item_count: usize) -> (i32, i32) {
    let Some(window) = web_sys::window() else {
        return (x, y);
    };
    let vw = window
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::MAX) as i32;
    let vh = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::MAX) as i32;
    // Estimated footprint: ~200px wide, 16px padding + ~28px per row.
    let menu_w = 200;
    let menu_h = 16 + 28 * item_count as i32;
    (
        x.clamp(0, (vw - menu_w - 4).max(4)),
        y.clamp(0, (vh - menu_h - 4).max(4)),
    )
}

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    let mut open = use_signal(|| false);
    let mut pos = use_signal(|| (0i32, 0i32));
    let items = props.items.clone();
    // Window keydown listener for Esc-to-dismiss; component-owned so it is
    // replaced on each open and dropped on unmount.
    let key_listener: Rc<
        RefCell<Option<(web_sys::Window, Closure<dyn FnMut(web_sys::KeyboardEvent)>)>>,
    > = use_hook(|| Rc::new(RefCell::new(None)));

    let key_listener_for_effect = key_listener.clone();
    let key_listener_for_drop = key_listener.clone();
    {
        use_effect(move || {
            if let Some((window, cb)) = key_listener_for_effect.borrow_mut().take() {
                let _ = window
                    .remove_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
            if !open() {
                return;
            }
            let Some(window) = web_sys::window() else {
                return;
            };
            let cb: Closure<dyn FnMut(web_sys::KeyboardEvent)> =
                Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
                    if e.key() == "Escape" {
                        open.set(false);
                    }
                }));
            let _ = window.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            *key_listener_for_effect.borrow_mut() = Some((window, cb));
        });
        use_drop(move || {
            if let Some((window, cb)) = key_listener_for_drop.borrow_mut().take() {
                let _ = window
                    .remove_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
        });
    }

    let clamped_pos = clamp_to_viewport(pos().0, pos().1, items.len());

    rsx! {
        div {
            style: "display: contents;",
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let c = e.data.client_coordinates();
                pos.set((c.x as i32, c.y as i32));
                open.set(true);
            },
            {props.children}
        }

        if open() {
            // backdrop to dismiss
            div {
                style: "position: fixed; inset: 0; z-index: 9590;",
                onclick: move |_| open.set(false),
                oncontextmenu: move |e: MouseEvent| { e.prevent_default(); open.set(false); },
            }
            div {
                class: "context-menu",
                style: "left: {clamped_pos.0}px; top: {clamped_pos.1}px;",
                for (i, item) in items.iter().enumerate() {
                    button {
                        key: "{i}",
                        class: if item.danger { "context-menu-item is-danger" } else { "context-menu-item" },
                        onclick: move |_| {
                            open.set(false);
                            props.on_select.call(i);
                        },
                        "{item.label}"
                    }
                }
            }
        }
    }
}
