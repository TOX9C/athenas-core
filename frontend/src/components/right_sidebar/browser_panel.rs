use dioxus::prelude::*;

/// Browser panel for the right sidebar.
/// Uses an iframe for embedded webview navigation.
#[component]
pub fn RightBrowserPanel() -> Element {
    let mut url = use_signal(|| "https://docs.rs".to_string());
    let mut url_input = use_signal(|| "https://docs.rs".to_string());

    let nav_btn = "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 12px;";

    let mut navigate = {
        let mut url_for_set = url;
        let url_for_read = url_input;
        move || {
            let target = url_for_read();
            url_for_set.set(target.clone());
        }
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%;",

            // Toolbar
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                // Back
                button {
                    style: "{nav_btn}",
                    title: "Back",
                    onclick: move |_| {
                        if let Some(window) = web_sys::window() {
                            if let Ok(history) = window.history() {
                                let _ = history.back();
                            }
                        }
                    },
                    "\u{2190}"
                }

                // Forward
                button {
                    style: "{nav_btn}",
                    title: "Forward",
                    onclick: move |_| {
                        if let Some(window) = web_sys::window() {
                            if let Ok(history) = window.history() {
                                let _ = history.forward();
                            }
                        }
                    },
                    "\u{2192}"
                }

                // Reload
                button {
                    style: "{nav_btn}",
                    title: "Reload",
                    onclick: move |_| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().reload();
                        }
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
                            navigate();
                        }
                    },
                    placeholder: "Enter URL..."
                }
            }

            // Webview via iframe
            div {
                style: "flex: 1; background: var(--bgTertiary);",
                iframe {
                    src: "{url()}",
                    style: "width: 100%; height: 100%; border: none;",
                    "sandbox": "allow-scripts allow-same-origin allow-forms allow-popups",
                }
            }

            // Recently opened
            div {
                style: "border-top: 1px solid var(--border); padding: 8px; background: var(--bgSecondary);",

                div {
                    style: "font-size: 9px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--textDim); margin-bottom: 4px;",
                    "Recently Opened"
                }

                for recent_url in ["https://docs.rs", "https://crates.io", "https://localhost:3000"].iter() {
                    div {
                        key: "{recent_url}",
                        style: "font-size: 11px; padding: 4px 8px; color: var(--textMuted); cursor: pointer; border-radius: 4px;",
                        onclick: move |_| {
                            url_input.set(recent_url.to_string());
                            url.set(recent_url.to_string());
                        },
                        "{recent_url}"
                    }
                }
            }
        }
    }
}
