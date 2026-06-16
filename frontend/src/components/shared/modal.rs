use super::icon::IconClose;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    pub title: String,
    pub on_close: EventHandler<()>,
    #[props(default = 480)]
    pub width: u32,
    pub children: Element,
    #[props(default)]
    pub footer: Option<Element>,
}

#[component]
pub fn Modal(props: ModalProps) -> Element {
    let width_str = format!("{}px", props.width);
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
                class: "modal-container modal-card",
                style: "background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-lg); width: {width_str}; max-width: 90vw; max-height: 82vh; display: flex; flex-direction: column; overflow: hidden;",
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
