//! Reusable controls shared by the settings sections.

use crate::components::shared::icon::{IconCheck, IconChevronDown};
use crate::themes::AVAILABLE_FONTS;
use crate::utils::font_size::{adjust_font_size, MAX_FONT_SIZE, MIN_FONT_SIZE};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

#[derive(Props, Clone, PartialEq)]
pub struct GroupLabelProps {
    pub label: &'static str,
    /// When true, suppresses the top margin (first group in a section).
    #[props(default)]
    pub first: bool,
}

#[component]
pub fn GroupLabel(props: GroupLabelProps) -> Element {
    let cls = if props.first {
        "group-label label-first"
    } else {
        "group-label"
    };
    rsx! {
        div { class: "{cls}",
            span { "{props.label}" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub(super) struct LabeledFieldProps {
    pub(super) label: &'static str,
    pub(super) description: Option<&'static str>,
    pub(super) children: Element,
}

#[component]
pub(super) fn LabeledField(props: LabeledFieldProps) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 4px; margin-bottom: 14px;",
            div {
                style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent);",
                "{props.label}"
            }
            if let Some(desc) = props.description {
                div {
                    style: "color: var(--textDim); font-size: 11px; padding-left: 12px;",
                    "{desc}"
                }
            }
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub(crate) struct ToggleProps {
    pub(crate) active: bool,
    pub(crate) on_toggle: EventHandler<MouseEvent>,
}

#[component]
pub(crate) fn Toggle(props: ToggleProps) -> Element {
    let cls = if props.active {
        "toggle is-active"
    } else {
        "toggle"
    };
    rsx! {
        button {
            class: "{cls}",
            r#type: "button",
            role: "switch",
            aria_checked: "{props.active}",
            onclick: move |e| props.on_toggle.call(e),
            div { class: "knob" }
        }
    }
}

/// Font-family dropdown popover. Each option is rendered in its own
/// typeface (`font-family` per option) so the user previews the face
/// they are about to pick. State (open/closed) is local to the dropdown.
///
/// The popover defaults closed and is local to this component. Selection
/// is one-way from child → parent via `on_select`. It dismisses on
/// outside-click or Escape (see the listener setup in the component body).
#[derive(Props, Clone, PartialEq)]
pub(super) struct FontDropdownProps {
    /// Current selection (the option rendered as the active affordance).
    pub(super) current: String,
    /// Called with the chosen family name when the user picks one.
    pub(super) on_select: EventHandler<String>,
}

#[component]
pub(super) fn FontDropdown(props: FontDropdownProps) -> Element {
    // Local signal: is the popover open?
    let mut open = use_signal(|| false);

    // Unique DOM id per instance — the settings panel and the settings modal
    // can both mount a FontDropdown, and the click-away listener needs to
    // know which one the pointer landed inside.
    let root_id = use_signal(|| format!("font-dropdown-{}", uuid::Uuid::new_v4()));

    // Local signal: the option count, used as the loop range. We keep it
    // as a `Vec<&'static str>` mirroring AVAILABLE_FONTS but capture it
    // inside the component as a local constant Vec — this avoids re-creating
    // the list on every render (use_signal(|| …) for closures is fine; this
    // is set once and never mutated).
    let fonts: Vec<&'static str> = AVAILABLE_FONTS.to_vec();

    // Window-level dismissal listeners. The slot owns the (window, closures)
    // pair so a stale pair is detached before a fresh one is installed, and
    // use_drop guarantees removal on unmount.
    let listener_slot: Rc<
        RefCell<
            Option<(
                web_sys::Window,
                Closure<dyn FnMut(web_sys::MouseEvent)>,
                Closure<dyn FnMut(web_sys::KeyboardEvent)>,
            )>,
        >,
    > = use_hook(|| Rc::new(RefCell::new(None)));
    {
        let listener_slot = listener_slot.clone();
        use_effect(move || {
            // Detach any previous pair (open→closed→open cycles).
            if let Some((window, mousedown_cb, keydown_cb)) = listener_slot.borrow_mut().take() {
                let _ = window.remove_event_listener_with_callback(
                    "mousedown",
                    mousedown_cb.as_ref().unchecked_ref(),
                );
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    keydown_cb.as_ref().unchecked_ref(),
                );
            }
            if !open() {
                return;
            }
            let Some(window) = web_sys::window() else {
                return;
            };
            let selector = format!("#{}", root_id.read());
            let mut open_for_close = open;

            // Outside-click: close when the pointer lands anywhere that is
            // not inside this dropdown's DOM subtree.
            let mousedown_cb = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
                let inside = event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .and_then(|el| el.closest(&selector).ok())
                    .flatten()
                    .is_some();
                if !inside {
                    open_for_close.set(false);
                }
            }) as Box<dyn FnMut(web_sys::MouseEvent)>);

            // Escape closes here and stops propagation so the settings modal's
            // own Escape handler doesn't also fire.
            let keydown_cb = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                if event.key() == "Escape" {
                    event.prevent_default();
                    event.stop_propagation();
                    open_for_close.set(false);
                }
            })
                as Box<dyn FnMut(web_sys::KeyboardEvent)>);

            // Capture phase so both run before Dioxus's delegated handlers.
            let _ = window.add_event_listener_with_callback_and_bool(
                "mousedown",
                mousedown_cb.as_ref().unchecked_ref(),
                true,
            );
            let _ = window.add_event_listener_with_callback_and_bool(
                "keydown",
                keydown_cb.as_ref().unchecked_ref(),
                true,
            );
            *listener_slot.borrow_mut() = Some((window, mousedown_cb, keydown_cb));
        });
    }
    let listener_slot_drop = listener_slot.clone();
    use_drop(move || {
        if let Some((window, mousedown_cb, keydown_cb)) = listener_slot_drop.borrow_mut().take() {
            let _ = window.remove_event_listener_with_callback(
                "mousedown",
                mousedown_cb.as_ref().unchecked_ref(),
            );
            let _ = window.remove_event_listener_with_callback(
                "keydown",
                keydown_cb.as_ref().unchecked_ref(),
            );
        }
    });

    rsx! {
        div {
            id: "{root_id}",
            div {
                class: if open() { "font-dropdown-afford is-open" } else { "font-dropdown-afford" },
                onclick: move |_| open.toggle(),
                span { class: "name", style: "font-family: '{props.current}', monospace;", "{props.current}" }
                span { class: "chevron",
                    IconChevronDown { size: Some(12), color: Some("currentColor".to_string()) }
                }
            }
            if open() {
                div { class: "font-dropdown-pop",
                    for (idx, font) in fonts.iter().enumerate() {
                        {
                            let font_str: &'static str = font;
                            // Compare two &str via deref-equality; the prior
                            // (*font_str == props.current.as_str()) compared
                            // a `str` (deref target) with a `&str` and broke.
                            let selected = *font_str == *props.current.as_str();
                            let font_for_click = font_str.to_string();
                            rsx! {
                                div {
                                    key: "{idx}",
                                    class: if selected { "font-dropdown-opt is-selected" } else { "font-dropdown-opt" },
                                    style: "font-family: '{font_str}', monospace;",
                                    onclick: move |_| {
                                        open.set(false);
                                        props.on_select.call(font_for_click.clone());
                                    },
                                    span { "{font_str}" }
                                    span { class: "check",
                                        IconCheck { size: Some(12), color: Some("currentColor".to_string()) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Font-size ± stepper. Value clamped to 10..=24 inclusive. Single click
/// only (no hold-to-repeat in v1).
#[derive(Props, Clone, PartialEq)]
pub(super) struct SizeStepperProps {
    pub(super) value: u8,
    pub(super) on_change: EventHandler<u8>,
}

#[component]
pub(super) fn SizeStepper(props: SizeStepperProps) -> Element {
    let step = |delta: i8| {
        move |_| {
            let next = adjust_font_size(props.value, delta);
            if next != props.value {
                props.on_change.call(next);
            }
        }
    };
    rsx! {
        div { class: "size-stepper",
            button {
                class: "size-step",
                r#type: "button",
                aria_label: "Decrease font size",
                disabled: props.value <= MIN_FONT_SIZE,
                onclick: step(-1),
                "−"
            }
            div { class: "size-step-value",
                span { class: "px", "{props.value}" }
                span { class: "unit", "PIXELS" }
            }
            button {
                class: "size-step",
                r#type: "button",
                aria_label: "Increase font size",
                disabled: props.value >= MAX_FONT_SIZE,
                onclick: step(1),
                "+"
            }
        }
    }
}
