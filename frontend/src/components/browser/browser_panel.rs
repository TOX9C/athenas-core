use super::browser_toolbar::BrowserToolbar;
use dioxus::prelude::*;

#[component]
pub fn BrowserPanel() -> Element {
    let mut url = use_signal(|| "https://docs.rs".to_string());
    let mut loading = use_signal(|| false);

    rsx! {
        div {
            class: "browser-panel",
            style: "height: 100%; display: flex; flex-direction: column; background: var(--bg); color: var(--text);",

            BrowserToolbar {
                url: url(),
                on_navigate: move |new_url: String| {
                    url.set(new_url);
                    loading.set(true);
                    // TODO: load URL via Tauri IPC
                },
                on_close: move |_| {
                    // TODO: close browser panel
                },
            }

            // Browser content (stub - replaces webview)
            div {
                style: "flex: 1; display: flex; align-items: center; justify-content: center; background: var(--bgTertiary);",

                if loading() {
                    span {
                        style: "font-size: 12px; color: var(--textDim);",
                        "Loading..."
                    }
                } else {
                    div {
                        style: "text-align: center; color: var(--textDim);",
                        span {
                            style: "font-size: 24px; font-weight: 700; opacity: 0.25; display: block; color: var(--accent);",
                            "WEB"
                        }
                        span { style: "font-size: 11px; margin-top: 8px; display: block;", "Browser view (stub)" }
                        span { style: "font-size: 10px; display: block; margin-top: 4px;", "TODO: embed webview via Tauri" }
                    }
                }
            }
        }
    }
}
