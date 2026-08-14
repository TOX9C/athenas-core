use dioxus::prelude::*;
use js_sys::Reflect;
use wasm_bindgen::JsCast;

use crate::components::shared::icon::{IconCopy, IconPlus};
use crate::components::shared::illustration::{EmptyArt, EmptyState};

/// Minimal skills panel -- text area for pasting definitions, list of skill names.
#[component]
pub fn SkillsPanel() -> Element {
    let mut skills = use_signal(Vec::<String>::new);
    let mut input_text = use_signal(String::new);

    let add_skill_name = move |_| {
        let text = input_text();
        let name = text.trim().to_string();
        if !name.is_empty() {
            skills.write().push(name);
            input_text.set(String::new());
        }
    };

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: 0; overflow: hidden;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                span { style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;", "AST" }
                span {
                    style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "Skills"
                }
            }

            // Add skill
            div {
                style: "padding: 8px 12px; border-bottom: 1px solid var(--border); display: flex; gap: 6px;",

                input {
                    class: "field",
                    style: "flex: 1;",
                    value: "{input_text}",
                    oninput: move |e| input_text.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            let text = input_text();
                            let name = text.trim().to_string();
                            if !name.is_empty() {
                                skills.write().push(name);
                                input_text.set(String::new());
                            }
                        }
                    },
                    placeholder: "Add skill name..."
                }

                button {
                    class: "icon-btn lit-sweep",
                    title: "Add skill",
                    onclick: add_skill_name,
                    IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                }
            }

            // Skills list
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px 12px;",

                if skills().is_empty() {
                    EmptyState {
                        kind: EmptyArt::Generic,
                        title: "No skills".to_string(),
                        hint: Some("Add skills to guide Athena.".to_string()),
                    }
                } else {
                    for (i, skill) in skills().iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "lit-sweep",
                            style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: var(--radius-sm);",

                            span {
                                style: "flex: 1; font-size: 11px; color: var(--textMuted);",
                                "{skill}"
                            }

                            {
                                let skill_name = skill.clone();
                                rsx! {
                                    button {
                                        class: "icon-btn",
                                        title: "Copy skill name",
                                        onclick: move |_| {
                                            let Some(window) = web_sys::window() else {
                                                web_sys::console::warn_1(&"[SkillsPanel] window unavailable while copying skill".into());
                                                return;
                                            };
                                            if let Ok(nav) = Reflect::get(&window, &wasm_bindgen::JsValue::from_str("navigator")) {
                                                if let Ok(cb) = Reflect::get(&nav, &wasm_bindgen::JsValue::from_str("clipboard")) {
                                                    if let Ok(write_text) = Reflect::get(&cb, &wasm_bindgen::JsValue::from_str("writeText")) {
                                                        if let Ok(fn_) = write_text.dyn_into::<js_sys::Function>() {
                                                            let _ = fn_.call1(&cb, &wasm_bindgen::JsValue::from_str(&skill_name));
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        IconCopy { size: Some(13), color: Some("currentColor".to_string()) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
