use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use crate::components::shared::icon::{IconArrowLeft, IconArrowRight, IconGlobe, IconRefresh};
use crate::tauri_bridge;

const BROWSER_ID: &str = "sidebar-browser";
const DEFAULT_URL: &str = "https://www.google.com";

/// Browser panel for the right sidebar.
///
/// Controls a native Tauri child webview that is overlaid at the right
/// sidebar area.  The iframe was replaced because `X-Frame-Options`
/// blocks all major sites (Google, GitHub, etc.).  The child webview
/// is a real WebKit process that lives *inside* the same window.
#[component]
pub fn RightBrowserPanel() -> Element {
    let url = use_signal(|| DEFAULT_URL.to_string());
    let mut url_input = use_signal(|| DEFAULT_URL.to_string());

    // Quick access URLs
    let quick_urls: Vec<(&str, &str)> = vec![
        ("Google", "https://www.google.com"),
        ("GitHub", "https://github.com"),
        ("Rust", "https://doc.rust-lang.org"),
        ("React", "https://react.dev"),
        ("VS Code Docs", "https://code.visualstudio.com/docs"),
    ];

    let localhost_urls: Vec<(&str, &str)> = vec![
        (":3000", "http://localhost:3000"),
        (":5173", "http://localhost:5173"),
        (":8080", "http://localhost:8080"),
        (":8000", "http://localhost:8000"),
        (":4200", "http://localhost:4200"),
        (":5000", "http://localhost:5000"),
        (":3001", "http://localhost:3001"),
        (":5174", "http://localhost:5174"),
    ];

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; min-height: 0;",

            // ── Toolbar ────────────────────────────────────────────────────
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                // Back
                button {
                    class: "icon-btn",
                    title: "Back",
                    onclick: move |_| {
                        let mut url_clone = url.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_back(BROWSER_ID).await {
                                Ok(new_url) => url_clone.set(new_url),
                                Err(e) => {
                                    web_sys::console::warn_1(&JsValue::from_str(&format!("Back: {:?}", e)));
                                }
                            }
                        });
                    },
                    IconArrowLeft { size: Some(16), color: Some("currentColor".to_string()) }
                }

                // Forward
                button {
                    class: "icon-btn",
                    title: "Forward",
                    onclick: move |_| {
                        let mut url_clone = url.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            match tauri_bridge::browser_forward(BROWSER_ID).await {
                                Ok(new_url) => url_clone.set(new_url),
                                Err(e) => {
                                    web_sys::console::warn_1(&JsValue::from_str(&format!("Forward: {:?}", e)));
                                }
                            }
                        });
                    },
                    IconArrowRight { size: Some(16), color: Some("currentColor".to_string()) }
                }

                // Reload
                button {
                    class: "icon-btn",
                    title: "Reload",
                    onclick: move |_| {
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ = tauri_bridge::browser_reload(BROWSER_ID).await;
                        });
                    },
                    IconRefresh { size: Some(16), color: Some("currentColor".to_string()) }
                }

                // URL bar
                input {
                    class: "field",
                    style: "flex: 1; min-width: 0;",
                    value: "{url_input}",
                    oninput: move |e| url_input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            let trimmed = url_input().trim().to_string();
                            if !trimmed.is_empty() {
                                let mut url_clone = url.clone();
                                let mut input_clone = url_input.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    match tauri_bridge::browser_navigate(BROWSER_ID, &trimmed).await {
                                        Ok(_) => {
                                            url_clone.set(trimmed.clone());
                                            input_clone.set(trimmed);
                                        }
                                        Err(e) => {
                                            web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e)));
                                        }
                                    }
                                });
                            }
                        }
                    },
                    placeholder: "Enter URL..."
                }

                // Open in New Window
                button {
                    style: "padding: 4px 8px; border-radius: 4px; border: none; background: transparent; color: var(--accent); cursor: pointer; font-size: 12px; transition: color 0.15s ease; display: flex; align-items: center; justify-content: center;",
                    title: "Open in native browser window",
                    onclick: move |_| {
                        let current = url();
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ = tauri_bridge::browser_open_external(&current).await;
                        });
                    },
                    span { style: "font-size: 14px;", "\u{279A}" }
                }
            }

            // ── Native webview content area (rendered by Tauri child webview) ──
            // The real web content is drawn natively at the right sidebar position
            // by the backend child webview, so this div is just a visual spacer.
            div {
                style: "flex: 1; background: var(--bgTertiary); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--textDim); padding: 24px; text-align: center; min-height: 0;",
                span {
                    style: "opacity: 0.4;",
                    IconGlobe { size: Some(40), color: Some("var(--textDim)".to_string()) }
                }
                div {
                    style: "font-family: var(--font-display); font-size: 18px; font-weight: 600; color: var(--text);",
                    "Native Browser Active"
                }
                div {
                    style: "font-size: 11px; max-width: 280px; color: var(--textMuted);",
                    "The browser content is rendered by a native webview at the right sidebar position. Use the URL bar or quick access buttons below."
                }
            }

            // ── Quick Access ───────────────────────────────────────────────
            div {
                style: "border-top: 1px solid var(--border); padding: 8px 12px; background: var(--bgSecondary); flex-shrink: 0; display: flex; flex-direction: column; gap: 8px;",

                // Quick Access row
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    div {
                        style: "font-family: var(--font-display); font-size: 12px; font-weight: 600; letter-spacing: 0.02em; color: var(--textDim); white-space: nowrap; min-width: 70px;",
                        "Quick Access"
                    }
                    div {
                        style: "display: flex; flex-wrap: wrap; gap: 4px; flex: 1;",
                        for (name, url_str) in quick_urls.iter().cloned() {
                            button {
                                class: "card is-interactive",
                                style: "padding: 3px 8px; font-size: 10px; cursor: pointer; white-space: nowrap;",
                                onclick: move |_| {
                                    let target = url_str.to_string();
                                    let mut url_clone = url.clone();
                                    let mut input_clone = url_input.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match tauri_bridge::browser_navigate(BROWSER_ID, &target).await {
                                            Ok(_) => {
                                                url_clone.set(target.clone());
                                                input_clone.set(target);
                                            }
                                            Err(e) => {
                                                web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e)));
                                            }
                                        }
                                    });
                                },
                                "{name}"
                            }
                        }
                    }
                }

                // Localhost row
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    div {
                        style: "font-family: var(--font-display); font-size: 12px; font-weight: 600; letter-spacing: 0.02em; color: var(--textDim); white-space: nowrap; min-width: 70px;",
                        "Localhost"
                    }
                    div {
                        style: "display: flex; flex-wrap: wrap; gap: 4px; flex: 1;",
                        for (label, url_str) in localhost_urls.iter().cloned() {
                            button {
                                class: "card is-interactive",
                                style: "padding: 3px 8px; font-size: 10px; cursor: pointer; white-space: nowrap;",
                                onclick: move |_| {
                                    let target = url_str.to_string();
                                    let mut url_clone = url.clone();
                                    let mut input_clone = url_input.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match tauri_bridge::browser_navigate(BROWSER_ID, &target).await {
                                            Ok(_) => {
                                                url_clone.set(target.clone());
                                                input_clone.set(target);
                                            }
                                            Err(e) => {
                                                web_sys::console::error_1(&JsValue::from_str(&format!("Navigate: {:?}", e)));
                                            }
                                        }
                                    });
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
