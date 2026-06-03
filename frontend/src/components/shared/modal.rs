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
            class: "modal-overlay",
            style: "position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: var(--shadow); backdrop-filter: blur(6px); ",
        onclick: move |_| props.on_close.call(()),
        div {
            class: "modal-container",
            role: "dialog",
            "aria-modal": "true",
            style: "background: var(--bgSecondary); border: 1px solid var(--border); border-radius: 12px; width: {width_str}; max-width: 90vw; max-height: 80vh; display: flex; flex-direction: column; overflow: hidden; box-shadow: var(--shadowLg), 0 0 0 1px var(--borderActive); ",
            onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "modal-header",
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; border-bottom: 1px solid var(--border);",

                    span {
                        style: "font-size: 13px; font-weight: 600; color: var(--text);",
                        "{props.title}"
                    }

                    button {
                        style: "padding: 4px; border-radius: 6px; border: none; background: transparent; cursor: pointer; color: var(--textDim); display: flex; align-items: center; justify-content: center; transition: color 0.15s;",
                        "aria-label": "Close dialog",
                        onmouseover: move |_| {},
                        onclick: move |_| props.on_close.call(()),
                        IconClose { size: Some(22), color: Some("var(--textDim)".to_string()) }
                    }
                }

                // Body
                div {
                    class: "modal-body",
                    style: "flex: 1; overflow-y: auto; padding: 16px;",

                    {props.children}
                }

                // Footer (rendered outside scrollable body)
                if let Some(footer) = props.footer {
                    div {
                        class: "modal-footer",
                        style: "flex-shrink: 0; padding: 12px 16px; border-top: 1px solid var(--border);",

                        {footer}
                    }
                }
            }
        }
    }
}
