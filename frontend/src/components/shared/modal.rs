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
            style: "position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,0.6); backdrop-filter: blur(4px);",
            onclick: move |_| { /* only close from inner click */ },

            div {
                class: "modal-container",
                style: "background: var(--bgSecondary, #141820); border: 1px solid var(--border, #2a303e); border-radius: 12px; width: {width_str}; max-width: 90vw; max-height: 80vh; display: flex; flex-direction: column; overflow: hidden; box-shadow: 0 25px 50px rgba(0,0,0,0.8);",

                // Header
                div {
                    class: "modal-header",
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; border-bottom: 1px solid var(--border);",

                    span {
                        style: "font-size: 13px; font-weight: 600; color: var(--text);",
                        "{props.title}"
                    }

                    button {
                        style: "padding: 4px; border-radius: 6px; border: none; background: transparent; cursor: pointer; color: var(--textDim); font-size: 16px; line-height: 1;",
                        onclick: move |_| props.on_close.call(()),
                        "\u{00d7}"
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
