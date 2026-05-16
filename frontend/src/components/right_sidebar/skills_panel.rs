use dioxus::prelude::*;
use js_sys::Reflect;
use wasm_bindgen::JsCast;

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
            style: "display: flex; flex-direction: column; height: 100%;",

            // Header
            div {
                style: "padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",
                span { style: "font-size: 13px; font-weight: 600; color: var(--text);", "Skills" }
            }

            // Add skill
            div {
                style: "padding: 8px 12px; border-bottom: 1px solid var(--border); display: flex; gap: 6px;",

                input {
                    style: "flex: 1; padding: 4px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 11px; outline: none;",
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
                    style: "padding: 4px 10px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; font-size: 11px; font-weight: 500; cursor: pointer;",
                    onclick: add_skill_name,
                    "+"
                }
            }

            // Skills list
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px 12px;",

                if skills().is_empty() {
                    div {
                        style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--textDim); font-size: 12px;",
                        "No skills defined yet"
                    }
                } else {
                    for (i, skill) in skills().iter().enumerate() {
                        div {
                            key: "{i}",
                            style: "display: flex; align-items: center; gap: 6px; padding: 4px 0;",

                            span {
                                style: "flex: 1; font-size: 11px; color: var(--textMuted);",
                                "{skill}"
                            }

                            {
                                let skill_name = skill.clone();
                                rsx! {
                                    button {
                                        style: "padding: 2px 6px; border-radius: 4px; border: none; background: var(--bgTertiary); color: var(--textDim); cursor: pointer; font-size: 9px;",
                                        onclick: move |_| {
                                            let window = web_sys::window().unwrap();
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
                                        "Copy"
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
