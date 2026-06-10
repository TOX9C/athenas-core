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

    rsx! {
        div {
            class: "modal-overlay modal-scrim",
            style: "position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: color-mix(in srgb, var(--bg) 72%, transparent);",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-container modal-card",
                role: "dialog",
                "aria-modal": "true",
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
                        onclick: move |_| props.on_close.call(()),
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
