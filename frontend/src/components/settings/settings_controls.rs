//! Reusable controls shared by the settings sections.

use crate::themes::AVAILABLE_FONTS;
use dioxus::prelude::*;

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
            aria_pressed: "{props.active}",
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
/// is one-way from child → parent via `on_select`.
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

    // Local signal: the option count, used as the loop range. We keep it
    // as a `Vec<&'static str>` mirroring AVAILABLE_FONTS but capture it
    // inside the component as a local constant Vec — this avoids re-creating
    // the list on every render (use_signal(|| …) for closures is fine; this
    // is set once and never mutated).
    let fonts: Vec<&'static str> = AVAILABLE_FONTS.to_vec();

    rsx! {
        div {
            // The popover currently closes only via a second click on the
            // affordance. Outside-click close was discussed (the spec kept
            // this as a deferred-for-v2 behavior — see `docs/superpowers/
            // specs/2026-07-12-settings-codex-redesign-design.md` §10) and
            // is not implemented yet. The Esc-key close path gets added with
            // a future outside-click global mousedown listener (mirroring
            // the existing IntersectionObserver pattern in
            // `xterm_mount.rs:765-834`).
            div {
                class: if open() { "font-dropdown-afford is-open" } else { "font-dropdown-afford" },
                onclick: move |_| open.toggle(),
                span { class: "name", style: "font-family: '{props.current}', monospace;", "{props.current}" }
                span { class: "chevron", "▾" }
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
                                    span { class: "check", "✓" }
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
            let next = (props.value as i16 + delta as i16).clamp(10, 24) as u8;
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
                disabled: props.value <= 10,
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
                disabled: props.value >= 24,
                onclick: step(1),
                "+"
            }
        }
    }
}
