use dioxus::prelude::*;
use js_sys::Reflect;
use wasm_bindgen::JsCast;

use crate::components::shared::icon::{IconClose, IconCopy, IconPlus};
use crate::components::shared::illustration::{EmptyArt, EmptyState};

/// Local skills scratchpad panel: a persistent list of skill names the user
/// wants to remember (for pasting into agent prompts/commands). Entries are
/// stored in the app's KV store so they survive restarts. There is no backend
/// skills system yet, so the copy is honest about scope: saved on-device,
/// shared with agents when the user copies a name.
#[component]
pub fn SkillsPanel() -> Element {
    let mut skills = use_signal(Vec::<String>::new);
    let mut input_text = use_signal(String::new);
    let mut loaded = use_signal(|| false);

    // Restore persisted skills once on mount.
    use_effect(move || {
        if loaded() {
            return;
        }
        loaded.set(true);
        let mut skills_store = skills;
        wasm_bindgen_futures::spawn_local(async move {
            match crate::tauri_bridge::store_get("skills").await {
                Ok(raw) if !raw.trim().is_empty() => {
                    if let Ok(list) = serde_json::from_str::<Vec<String>>(&raw) {
                        skills_store.set(list);
                    }
                }
                _ => {}
            }
        });
    });

    // Write-through persistence on every change (add/remove/restore).
    use_effect(move || {
        let snapshot = skills.read().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "[]".to_string());
            let _ = crate::tauri_bridge::store_set("skills", &json).await;
        });
    });

    let mut add_skill = move |name: String| {
        let name = name.trim().to_string();
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
                span {
                    style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "Skills"
                }
                span {
                    style: "margin-left: auto; font-size: var(--text-2xs); color: var(--textDim); letter-spacing: 0.04em; text-transform: uppercase;",
                    "Local"
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
                            add_skill(input_text());
                        }
                    },
                    placeholder: "Add a skill name…"
                }

                button {
                    class: "icon-btn lit-sweep",
                    title: "Add skill",
                    onclick: move |_| add_skill(input_text()),
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
                        hint: Some("Names you add are saved on this device. Copy one to share it with an agent.".to_string()),
                    }
                } else {
                    for (i, skill) in skills().iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "lit-sweep",
                            style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: var(--radius-sm);",

                            span {
                                style: "flex: 1; font-size: 11px; color: var(--textMuted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
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

                            button {
                                class: "icon-btn",
                                title: "Remove skill",
                                onclick: move |_| {
                                    skills.write().remove(i);
                                },
                                IconClose { size: Some(13), color: Some("var(--textDim)".to_string()) }
                            }
                        }
                    }
                }
            }
        }
    }
}
