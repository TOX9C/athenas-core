use super::icon::IconClose;
use dioxus::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Number of mounted root-level modal overlays. Native child webviews must be
/// parked while any shared modal is present because CSS z-index cannot cover a
/// platform-native child webview on macOS.
pub fn provide_modal_overlay_store() {
    use_context_provider(|| Signal::new(0u32));
}

pub fn use_modal_overlay_store() -> Signal<u32> {
    use_context::<Signal<u32>>()
}

fn acquire_modal_overlay(mut overlay_count: Signal<u32>, mounted: Rc<Cell<bool>>) {
    if !mounted.replace(true) {
        let current = *overlay_count.read();
        overlay_count.set(current.saturating_add(1));
    }
}

fn release_modal_overlay(mut overlay_count: Signal<u32>, mounted: Rc<Cell<bool>>) {
    if mounted.replace(false) {
        let current = *overlay_count.read();
        overlay_count.set(current.saturating_sub(1));
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    pub title: String,
    pub on_close: EventHandler<()>,
    #[props(default = 480)]
    pub width: u32,
    /// Compact dialogs size to their content instead of inheriting the tall
    /// settings-modal footprint.
    #[props(default = false)]
    pub compact: bool,
    pub children: Element,
    #[props(default)]
    pub footer: Option<Element>,
}

#[component]
pub fn Modal(props: ModalProps) -> Element {
    let overlay_count = use_modal_overlay_store();
    let mounted = use_hook(|| Rc::new(Cell::new(false)));

    use_effect({
        let mounted = mounted.clone();
        move || acquire_modal_overlay(overlay_count, mounted.clone())
    });
    use_drop({
        let mounted = mounted.clone();
        move || release_modal_overlay(overlay_count, mounted)
    });

    let width_str = format!("{}px", props.width);
    let height_style = if props.compact {
        "height: auto; max-height: 70vh;"
    } else {
        "height: 82vh;"
    };
    let on_close = props.on_close;

    rsx! {
        div {
            class: "modal-overlay modal-scrim",
            // role+aria on the overlay let assistive tech announce the dialog.
            // tabindex lets the overlay receive keyboard focus (and thus the
            // onkeydown Escape handler below); without it, Escape never fires
            // because focus stays on the trigger element behind the modal.
            role: "dialog",
            "aria-modal": "true",
            "aria-label": "{props.title}",
            tabindex: "-1",
            style: "position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: color-mix(in srgb, var(--bg) 72%, transparent); outline: none;",
            onclick: move |_| on_close.call(()),
            // Escape closes — standard WAI-ARIA dialog behavior. Previously the
            // modal could only be dismissed by clicking the backdrop or X,
            // which is hostile to keyboard users.
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    e.prevent_default();
                    on_close.call(());
                }
            },
            div {
                class: if props.compact { "modal-container modal-card is-compact" } else { "modal-container modal-card" },
                style: "background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-lg); width: {width_str}; max-width: 90vw; {height_style} display: flex; flex-direction: column; overflow: hidden;",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "modal-header",
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 14px 18px; border-bottom: 1px solid var(--border);",

                    span {
                        style: "font-family: var(--font-display); font-size: 19px; font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                        "{props.title}"
                    }

                    button {
                        class: "icon-btn",
                        "aria-label": "Close dialog",
                        onclick: move |_| on_close.call(()),
                        IconClose { size: Some(16), color: Some("currentColor".to_string()) }
                    }
                }

                // Body
                div {
                    class: "modal-body",
                    style: "flex: 1; overflow-y: auto; padding: 18px;",
                    {props.children}
                }

                // Footer (rendered outside scrollable body)
                if let Some(footer) = props.footer {
                    div {
                        class: "modal-footer",
                        style: "flex-shrink: 0; padding: 14px 18px; border-top: 1px solid var(--border); display: flex; align-items: center; justify-content: flex-end; gap: 8px;",
                        {footer}
                    }
                }
            }
        }
    }
}
