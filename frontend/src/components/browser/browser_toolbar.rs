use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct BrowserToolbarProps {
    pub url: String,
    pub on_navigate: EventHandler<String>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn BrowserToolbar(props: BrowserToolbarProps) -> Element {
    let mut url_input = use_signal(|| props.url.clone());

    rsx! {
        div {
            class: "browser-toolbar",
            style: "display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

            // Back
            button {
                style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px;",
                onclick: move |_| {
                    // TODO: go back
                },
                "\u{2190}"
            }

            // Forward
            button {
                style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px;",
                onclick: move |_| {
                    // TODO: go forward
                },
                "\u{2192}"
            }

            // Reload
            button {
                style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px;",
                onclick: move |_| {
                    // TODO: reload
                },
                "\u{21bb}"
            }

            // URL bar
            input {
                style: "flex: 1; padding: 4px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 11px; outline: none;",
                value: "{url_input}",
                oninput: move |e| url_input.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter {
                        let url = url_input();
                        props.on_navigate.call(url);
                    }
                },
                placeholder: "Enter URL..."
            }

            // External link
            button {
                style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px;",
                onclick: move |_| {
                    // TODO: open in external browser
                },
                "\u{2197}"
            }

            // Close
            button {
                style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px;",
                onclick: move |_| props.on_close.call(()),
                "\u{00d7}"
            }
        }
    }
}
