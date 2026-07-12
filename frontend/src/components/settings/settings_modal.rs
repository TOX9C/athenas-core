use super::shortcuts_ref::ShortcutsRef;
use super::theme_picker::ThemePicker;
use crate::components::shared::modal::Modal;
use crate::stores::athena::use_athena_store;
use crate::stores::ui::use_ui_store;
use crate::themes::AVAILABLE_FONTS;
use dioxus::prelude::*;

/* =============================================================
SettingsContent – the codex of settings (six sections + floating index)
============================================================= */

#[component]
pub fn SettingsContent() -> Element {
    // 0..=5 — the topmost visible section index. Updated by the scroll
    // listener (Task 7). Initial value 0 (General) so the index shows
    // item one as active before the first scroll event.
    let mut active_idx = use_signal(|| 0u8);

    let section_i = rsx! {
        CodexSection {
            numeral: "I",
            title: "General",
            epigraph: "The forge — where the type is struck.",
            intro: Some("Configure your Athena environment — type, size, pane titles."),
            id: "s-i",
            GeneralSettings {}
        }
    };
    let section_ii = rsx! {
        CodexSection {
            numeral: "II",
            title: "Athena",
            epigraph: "The oracle — to whom the questions are put.",
            intro: Some("Configure your LLM provider. Works with any OpenAI-compatible API or Anthropic."),
            id: "s-ii",
            AthenaSettings {}
        }
    };
    let section_iii = rsx! {
        CodexSection {
            numeral: "III",
            title: "Agents",
            epigraph: "The order — those who act on your behalf.",
            intro: Some("Manage custom agents with aliases and commands that launch them."),
            id: "s-iii",
            AgentsSettings {}
        }
    };
    let section_iv = rsx! {
        CodexSection {
            numeral: "IV",
            title: "Themes",
            epigraph: "The aspect — how the temple catches the light.",
            intro: Some("Choose a color scheme for your Athena environment."),
            id: "s-iv",
            ThemePicker {}
        }
    };
    let section_v = rsx! {
        CodexSection {
            numeral: "V",
            title: "Shortcuts",
            epigraph: "The craft — the gestures by which the hand moves.",
            intro: Some("Quick reference for the most common keyboard shortcuts in Athena."),
            id: "s-v",
            ShortcutsRef {}
        }
    };
    let section_vi = rsx! {
        CodexSection {
            numeral: "VI",
            title: "About",
            epigraph: "The keystone — the temple knows itself.",
            intro: Some(""),
            id: "s-vi",
            AboutSettings {}
        }
    };

    let sections = [section_i, section_ii, section_iii, section_iv, section_v, section_vi];
    let numerals: [&'static str; 6] = ["I", "II", "III", "IV", "V", "VI"];
    let glyphs: [&'static str; 6] = [
        "\u{2699}", "\u{03A6}", "\u{232C}", "\u{25D1}", "\u{2318}", "\u{0398}",
    ];
    // U+2699 GEAR, U+03A6 PHI, U+232C BENZENE, U+25D1 CIRCLE WITH LEFT
    // HALF BLACK, U+2318 PLACE OF INTEREST SIGN, U+0398 GREEK CAPITAL
    // THETA. Glyphs render via Unicode in --font-display (Cormorant).

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; overflow: hidden; background: var(--bgSecondary); color: var(--text);",

            /* ── Interior masthead (decorative; modal close button is owned by Modal) ── */
            div {
                style: "display: flex; align-items: center; gap: 12px; padding: 18px 24px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",
                span {
                    style: "width: 30px; height: 30px; border-radius: 50%; border: 1px solid var(--accent); display: inline-flex; align-items: center; justify-content: center; color: var(--accent); font-family: var(--font-display); font-size: 18px; box-shadow: 0 0 10px var(--accentSubtle), inset 0 0 6px var(--accentSubtle);",
                    "\u{0398}"
                }
                span {
                    style: "font-family: var(--font-display); font-size: 20px; font-weight: 600; color: var(--accent); letter-spacing: 0.03em;",
                    "Codex of Settings"
                }
                span {
                    style: "margin-left: auto; color: var(--textDim); font-size: 10px; letter-spacing: 0.16em; text-transform: uppercase;",
                    "Bronze Relief \u{2022} ✦"
                }
            }

            /* ── Body: index on the left, scroll tome on the right ── */
            div {
                style: "display: flex; flex: 1; min-height: 0;",

                /* Floating left index (sticky) */
                div { class: "codex-index",
                    for (idx, _section) in sections.iter().enumerate() {
                        {
                            let idx_u8 = idx as u8;
                            let active = active_idx() == idx_u8;
                            let cls = if active { "codex-index-item is-active" } else { "codex-index-item" };
                            let onidx = active_idx.clone();
                            let section_id = match idx_u8 {
                                0 => "s-i",
                                1 => "s-ii",
                                2 => "s-iii",
                                3 => "s-iv",
                                4 => "s-v",
                                _ => "s-vi",
                            };
                            let section_id_for_click = section_id.to_string();
                            rsx! {
                                button {
                                    key: "{idx}",
                                    class: "{cls}",
                                    r#type: "button",
                                    aria_label: "Jump to section {numerals[idx]}",
                                    onclick: move |_| {
                                        onidx.set(idx_u8);
                                        if let Some(window) = web_sys::window() {
                                            if let Some(doc) = window.document() {
                                                if let Some(el) = doc.get_element_by_id(&section_id_for_click) {
                                                    let _ = el.scroll_into_view();
                                                }
                                            }
                                        }
                                    },
                                    span { "{numerals[idx]}" }
                                    span { class: "glyph", "{glyphs[idx]}" }
                                }
                            }
                        }
                    }
                }

                /* Scroll tome */
                div {
                    id: "codex-tome-scroll",
                    class: "codex-tome",
                    for (idx, section) in sections.iter().enumerate() {
                        // Each element is already a CodexSection-wrapped <section> with id s-i..s-vi.
                        section
                    }
                }
            }
        }
    }
}

/* =============================================================
SettingsModal – wraps SettingsContent in a modal overlay
============================================================= */

#[derive(Props, Clone, PartialEq)]
pub struct SettingsModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn SettingsModal(props: SettingsModalProps) -> Element {
    rsx! {
        Modal {
            title: "Settings",
            on_close: move |_| props.on_close.call(()),
            width: 860,
            SettingsContent {}
        }
    }
}

/* =============================================================
Tab: General
============================================================= */

#[component]
fn GeneralSettings() -> Element {
    let mut ui_state = use_ui_store();

    rsx! {
        GroupLabel { label: "Typography", first: true }

        LabeledField {
            label: "Font Family",
            description: Some("Choose your monospace typeface for the editor and terminal."),
            FontDropdown {
                current: ui_state.read().font_family.clone(),
                on_select: move |family: String| {
                    let size = ui_state.read().font_size;
                    ui_state.write().font_family = family.clone();
                    crate::themes::apply_font_to_dom(&family, size);
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = crate::tauri_bridge::store_set("font_family", &family).await;
                    });
                }
            }
        }

        LabeledField {
            label: "Font Size",
            description: Some("Adjust the base font size used throughout the interface and terminal."),
            SizeStepper {
                value: ui_state.read().font_size,
                on_change: move |val: u8| {
                    let fam = ui_state.read().font_family.clone();
                    ui_state.write().font_size = val;
                    crate::themes::apply_font_to_dom(&fam, val);
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = crate::tauri_bridge::store_set("font_size", &val.to_string()).await;
                    });
                }
            }
        }

        /* ── Live preview ── */
        div { class: "settings-preview",
            div { class: "settings-preview-tabs",
                span { class: "settings-preview-dot", style: "background: var(--error);" }
                span { class: "settings-preview-dot", style: "background: #d2973c;" }
                span { class: "settings-preview-dot", style: "background: var(--success);" }
                span { class: "settings-preview-tag", "Preview" }
            }
            div {
                class: "settings-preview-code",
                style: "font-family: '{ui_state.read().font_family}', monospace; font-size: {ui_state.read().font_size}px;",
                r#"fn main() {{
    println!("Hello, world!");
}}"#
            }
        }

        GroupLabel { label: "Pane Titles" }

        /* ── Smart pane titles label + toggle row ── */
        div {
            style: "display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 14px 16px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); margin-bottom: 8px;",
            div {
                style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
                span {
                    style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent);",
                    "Smart pane titles"
                }
                span {
                    style: "font-size: 11px; color: var(--textDim);",
                    "Auto-generate names for idle shells and summarize agent titles via LLM."
                }
            }
            Toggle {
                active: ui_state.read().smart_pane_titles,
                on_toggle: move |_| {
                    let next = !ui_state.read().smart_pane_titles;
                    ui_state.write().smart_pane_titles = next;
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = crate::tauri_bridge::store_set(
                            "smart_pane_titles",
                            if next { "true" } else { "false" },
                        )
                        .await;
                    });
                }
            }
        }
    }
}

/* =============================================================
Tab: Athena
============================================================= */

#[component]
fn AthenaSettings() -> Element {
    let mut api_key_set = use_signal(|| false);
    let mut api_key_input = use_signal(String::new);
    let mut base_url = use_signal(|| "https://api.openai.com/v1".to_string());
    let mut model = use_signal(|| "gpt-4o".to_string());
    let mut save_status = use_signal(|| Option::<bool>::None);
    let mut save_error = use_signal(String::new);
    let mut athena_state = use_athena_store();
    let mut toast_store = crate::components::shared::toast::use_toast_store();

    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(status) = crate::tauri_bridge::store_get("llm.api_key").await {
                api_key_set.set(status == "set");
            }
            if let Ok(url) = crate::tauri_bridge::store_get("llm.base_url").await {
                base_url.set(url);
            }
            if let Ok(m) = crate::tauri_bridge::store_get("llm.model").await {
                model.set(m);
            }
        });
    });

    let mut do_save = move || {
        let new_key = api_key_input.read().clone();
        let url = base_url.read().clone();
        let m = model.read().clone();
        let key_will_be_set = !new_key.is_empty() || api_key_set();
        api_key_input.set(String::new());

        wasm_bindgen_futures::spawn_local(async move {
            let mut key_ok = key_will_be_set;
            let mut key_err = String::new();

            if !new_key.is_empty() {
                match crate::tauri_bridge::store_set("llm.api_key", &new_key).await {
                    Ok(()) => {
                        api_key_set.set(true);
                        key_ok = true;
                    }
                    Err(e) => {
                        key_ok = false;
                        key_err = format!("{:?}", e);
                        web_sys::console::error_1(
                            &format!("[AthenaSettings] Failed to save API key: {:?}", e).into(),
                        );
                    }
                }
            }
            if let Err(e) = crate::tauri_bridge::store_set("llm.base_url", &url).await {
                key_err = if key_err.is_empty() {
                    format!("base URL: {:?}", e)
                } else {
                    format!("{}; base URL: {:?}", key_err, e)
                };
            }
            if let Err(e) = crate::tauri_bridge::store_set("llm.model", &m).await {
                key_err = if key_err.is_empty() {
                    format!("model: {:?}", e)
                } else {
                    format!("{}; model: {:?}", key_err, e)
                };
            }

            let any_error = !key_err.is_empty();
            save_error.set(key_err.clone());
            save_status.set(Some(!any_error));

            if any_error {
                toast_store.write().push(crate::components::shared::toast::Toast {
                    id: format!("athena-save-{}", chrono::Utc::now().timestamp_millis()),
                    toast_type: crate::components::shared::toast::ToastType::Error,
                    title: "Failed to save Athena settings".to_string(),
                    message: format!(
                        "The API key could not be stored. Check that OS keychain access is allowed. ({})",
                        key_err
                    ),
                    duration_ms: 6000,
                });
            }

            athena_state.write().set_api_configured(Some(key_ok));
            athena_state
                .write()
                .set_configured_model(if m.trim().is_empty() { None } else { Some(m) });
        });
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 24px; max-width: 620px;",

            SectionHeader { title: "Athena", desc: "Configure your LLM provider. Athena works with any OpenAI-compatible API or Anthropic." }

            div {
                style: "display: flex; flex-direction: column; gap: 16px;",

                // API Key
                div {
                    style: "display: flex; flex-direction: column; gap: 8px;",
                    SettingsSection { label: "API Key", description: None }
                    div {
                        style: "display: flex; align-items: center; gap: 8px; margin-top: 4px;",
                        div {
                            style: "font-size: var(--text-xs); color: var(--textMuted); font-family: var(--fontFamily); background: var(--bgTertiary); padding: 4px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border);",
                            if api_key_set() {
                                "●●●● Set"
                            } else {
                                "Not set"
                            }
                        }
                        button {
                            class: "btn-secondary btn-sm",
                            style: "padding: 4px 10px; font-size: var(--text-xs); font-weight: 500;",
                            title: "Test keyring access",
                            onclick: move |_| {
                                let mut toast = toast_store.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    match crate::tauri_bridge::test_llm_api_key().await {
                                        Ok(json) => {
                                            let parsed: serde_json::Value = match serde_json::from_str(&json) {
                                                Ok(v) => v,
                                                Err(_) => {
                                                    toast.write().push(
                                                        crate::components::shared::toast::Toast {
                                                            id: format!("test-key-{}", chrono::Utc::now().timestamp_millis()),
                                                            toast_type: crate::components::shared::toast::ToastType::Warning,
                                                            title: "Key test error".to_string(),
                                                            message: "Could not parse key test response".to_string(),
                                                            duration_ms: 4000,
                                                        }
                                                    );
                                                    return;
                                                }
                                            };
                                            let ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                                            let msg = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown response");
                                            toast.write().push(
                                                crate::components::shared::toast::Toast {
                                                    id: format!("test-key-{}", chrono::Utc::now().timestamp_millis()),
                                                    toast_type: if ok {
                                                        crate::components::shared::toast::ToastType::Success
                                                    } else {
                                                        crate::components::shared::toast::ToastType::Warning
                                                    },
                                                    title: if ok { "Key OK".to_string() } else { "Key test failed".to_string() },
                                                    message: msg.to_string(),
                                                    duration_ms: 6000,
                                                }
                                            );
                                        }
                                        Err(e) => {
                                            toast.write().push(
                                                crate::components::shared::toast::Toast {
                                                    id: format!("test-key-{}", chrono::Utc::now().timestamp_millis()),
                                                    toast_type: crate::components::shared::toast::ToastType::Error,
                                                    title: "Key test failed".to_string(),
                                                    message: format!("{:?}", e),
                                                    duration_ms: 5000,
                                                }
                                            );
                                        }
                                    }
                                });
                            },
                            "Test Key"
                        }
                    }
                    input {
                        value: "{api_key_input}",
                        r#type: "password",
                        class: "field",
                        style: "width: 100%; box-sizing: border-box; margin-top: 8px;",
                        placeholder: "Enter new API key…",
                        oninput: move |e| { api_key_input.set(e.value()); save_status.set(None); },
                    }
                }

                // Base URL
                div {
                    style: "display: flex; flex-direction: column; gap: 8px;",
                    SettingsSection { label: "Base URL", description: Some("e.g. https://api.openai.com/v1, https://api.groq.com/openai/v1, http://localhost:1234/v1".to_string()) }
                    input {
                        value: "{base_url}",
                        class: "field",
                        style: "width: 100%; box-sizing: border-box;",
                        placeholder: "https://api.openai.com/v1",
                        oninput: move |e| { base_url.set(e.value()); save_status.set(None); },
                    }
                }

                // Model
                div {
                    style: "display: flex; flex-direction: column; gap: 8px;",
                    SettingsSection { label: "Model", description: Some("gpt-4o, claude-sonnet-4-6, llama3.1, ...".to_string()) }
                    input {
                        value: "{model}",
                        class: "field",
                        style: "width: 100%; box-sizing: border-box;",
                        placeholder: "gpt-4o, gpt-4, llama3.1, ...",
                        oninput: move |e| { model.set(e.value()); save_status.set(None); },
                    }
                }

                // Save button
                div {
                    style: "display: flex; align-items: center; gap: 12px; margin-top: 4px; padding-top: 12px; border-top: 1px solid var(--border);",
                    button {
                        class: "btn-primary",
                        onclick: move |_| {
                            save_status.set(None);
                            save_error.set(String::new());
                            do_save();
                        },
                        "Save"
                    }
                    match save_status() {
                        Some(true) => rsx! {
                            span {
                                style: "display: flex; align-items: center; gap: 4px; font-size: var(--text-xs); color: var(--success); font-weight: 500;",
                                "✓ Saved"
                            }
                        },
                        Some(false) => rsx! {
                            span {
                                style: "display: flex; align-items: center; gap: 4px; font-size: var(--text-xs); color: var(--error); font-weight: 500;",
                                "✕ Save failed — see notification"
                            }
                        },
                        None => rsx! {},
                    }
                }
            }
        }
    }
}

/* =============================================================
Tab: Agents
============================================================= */

#[component]
fn AgentsSettings() -> Element {
    let mut ui_state = use_ui_store();
    let mut new_alias = use_signal(String::new);
    let mut new_command = use_signal(String::new);
    let mut new_is_claude = use_signal(|| false);
    let mut new_priority = use_signal(|| false);
    let mut show_form = use_signal(|| false);

    let _agents_snapshot: Vec<crate::types::workspace::CustomAgent> =
        ui_state.read().custom_agents.clone();

    let persist = |agents: &[_]| {
        let a = agents.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(json) = serde_json::to_string(&a) {
                let _ = crate::tauri_bridge::store_set("custom_agents", &json).await;
            }
        });
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 24px; max-width: 620px;",

            SectionHeader { title: "Agents", desc: "Manage your agent configurations. Create custom agents with aliases and commands that launch them." }

            // New Agent form
            div {
                style: "display: flex; flex-direction: column; gap: 16px;",

                div {
                    style: "display: flex; align-items: center; justify-content: space-between;",
                    div {
                        style: "display: flex; align-items: center; gap: 6px;",
                        div {
                            style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                            "Custom Agents"
                        }
                    }
                    if !show_form() {
                        button {
                            class: "btn-secondary btn-sm",
                            style: "font-weight: 500; padding: 6px 14px;",
                            onclick: move |_| { show_form.set(true); new_alias.set(String::new()); new_command.set(String::new()); new_is_claude.set(false); new_priority.set(false); },
                            "+ Add Agent"
                        }
                    }
                }

                if show_form() {
                    div {
                        class: "card",
                        style: "display: flex; flex-direction: column; gap: 14px; padding: 20px;",
                        div {
                            style: "display: flex; align-items: center; gap: 6px; margin-bottom: 4px;",
                            div {
                                style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                                "New Agent"
                            }
                        }
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px;",
                            input {
                                class: "field",
                                style: "width: 100%; box-sizing: border-box;",
                                value: "{new_alias}",
                                placeholder: "Alias (e.g., my-claude)",
                                oninput: move |e| new_alias.set(e.value()),
                            }
                            input {
                                class: "field",
                                style: "width: 100%; box-sizing: border-box;",
                                value: "{new_command}",
                                placeholder: "Command (e.g., claude --project foo)",
                                oninput: move |e| new_command.set(e.value()),
                            }
                        }
                        // Treat as Claude toggle
                        div {
                            style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; cursor: pointer; user-select: none; padding: 6px 0;",
                            onclick: move |_| new_is_claude.set(!new_is_claude()),
                            div {
                                style: "display: flex; flex-direction: column; gap: 2px; min-width: 0; padding-right: 8px;",
                                span {
                                    style: "display: flex; align-items: center; gap: 6px; font-family: var(--font-ui); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                                    "Treat as Claude"
                                }
                                span {
                                    style: "font-family: var(--font-ui); font-size: var(--text-xs); color: var(--textDim); padding-left: 14px;",
                                    "Show resume variants + running detection"
                                }
                            }
                            CustomToggle { active: new_is_claude() }
                        }
                        // Priority toggle
                        if new_is_claude() {
                            div {
                                style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; cursor: pointer; user-select: none; padding: 10px 0 2px 0; margin-top: 4px; border-top: 1px solid var(--border);",
                                onclick: move |_| new_priority.set(!new_priority()),
                                div {
                                    style: "display: flex; flex-direction: column; gap: 2px; min-width: 0; padding-right: 8px;",
                                    span {
                                        style: "display: flex; align-items: center; gap: 6px; font-family: var(--font-ui); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                                        "Set as Priority"
                                    }
                                    span {
                                        style: "font-family: var(--font-ui); font-size: var(--text-xs); color: var(--textDim); padding-left: 14px;",
                                        "Default resume option for Claude sessions"
                                    }
                                }
                                CustomToggle { active: new_priority() }
                            }
                        }
                        div {
                            style: "display: flex; gap: 10px; justify-content: flex-end; margin-top: 4px; padding-top: 10px; border-top: 1px solid var(--border);",
                            button {
                                class: "btn-ghost btn-sm",
                                style: "font-weight: 500;",
                                onclick: move |_| show_form.set(false),
                                "Cancel"
                            }
                            button {
                                class: "btn-primary btn-sm",
                                style: if new_alias.read().trim().is_empty() || new_command.read().trim().is_empty() {
                                    "opacity: 0.5; cursor: not-allowed;"
                                } else {
                                    ""
                                },
                                onclick: move |_| {
                                    let alias = new_alias.read().trim().to_string();
                                    let cmd = new_command.read().trim().to_string();
                                    if alias.is_empty() || cmd.is_empty() { return; }
                                    let is_claude = new_is_claude();
                                    let priority = new_priority();
                                    let new_agent = crate::types::workspace::CustomAgent {
                                        id: format!("custom-{}", js_sys::Date::now() as u64),
                                        alias: alias,
                                        command: cmd,
                                        is_claude,
                                        priority,
                                    };
                                    let mut ag = ui_state.read().custom_agents.clone();
                                    if priority {
                                        for a in &mut ag { a.priority = false; }
                                    }
                                    ag.push(new_agent);
                                    let agc = ag.clone();
                                    ui_state.write().custom_agents = ag;
                                    persist(&agc);
                                    show_form.set(false);
                                },
                                "Save"
                            }
                        }
                    }
                }
            }

            // Render the custom agents list
            div {
                style: "display: flex; flex-direction: column; gap: 6px; margin-top: 6px;",
                CustomAgentList {}
            }

            // Predefined agents (read-only view)
            div {
                style: "margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border);",
                div {
                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 8px;",
                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                        "Built-in Agents"
                    }
                }
                div {
                    style: "display: flex; flex-direction: column; gap: 6px;",
                    for (name, cmd) in [
                        ("Claude Code", "claude"),
                        ("Codex", "codex"),
                        ("OpenCode", "opencode"),
                        ("Gemini CLI", "gemini"),
                        ("Shell", "Interactive shell"),
                    ] {
                        div {
                            key: "{name}",
                            class: "lit-sweep",
                            style: "display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary);",
                            span {
                                style: "font-size: var(--text-sm); font-weight: 500; color: var(--text);",
                                "{name}"
                            }
                            span {
                                style: "font-size: var(--text-xs); color: var(--textDim); font-family: var(--fontFamily); background: var(--bgTertiary); padding: 2px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border);",
                                "{cmd}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// Component that renders the list of custom agents from the store
#[component]
fn CustomAgentList() -> Element {
    let ui_state = use_ui_store();
    let agents_len = ui_state.read().custom_agents.len();

    if agents_len == 0 {
        return rsx! {
            div {
                style: "padding: 32px; text-align: center; color: var(--textDim); font-size: var(--text-xs); border: 1px dashed var(--border); border-radius: var(--radius-md); background: var(--bgSecondary);",
                "No custom agents yet. Click + Add Agent to create one."
            }
        };
    }

    rsx! {
        for i in 0..agents_len {
            {
                let agent = ui_state.read().custom_agents[i].clone();
                rsx! {
                    CustomAgentRow { agent }
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct CustomAgentRowProps {
    agent: crate::types::workspace::CustomAgent,
}

#[component]
fn CustomAgentRow(props: CustomAgentRowProps) -> Element {
    let mut ui_state = use_ui_store();
    let id = props.agent.id.clone();
    let alias = props.agent.alias.clone();
    let cmd = props.agent.command.clone();
    let agent_id_for_delete = id.clone();

    rsx! {
        div {
            key: "{id}",
            class: "lit-sweep",
            style: "display: flex; flex-direction: column; gap: 6px; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); transition: border-color 0.18s ease;",
            onmouseenter: move |_| {},
            div {
                style: "display: flex; align-items: center; justify-content: space-between; gap: 8px;",
                div {
                    style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                    span {
                        style: "font-size: var(--text-sm); font-weight: 600; color: var(--accent); background: var(--accentSubtle); padding: 3px 10px; border-radius: var(--radius-sm); border: 1px solid var(--accent);",
                        "{alias}"
                    }
                    if props.agent.is_claude {
                        span {
                            class: "badge",
                            style: "background: var(--accentSubtle); color: var(--accent); border: 1px solid var(--accent); font-size: var(--text-2xs); padding: 2px 8px;",
                            title: "Treated as Claude for resume + running detection",
                            "Claude"
                        }
                    }
                    if props.agent.priority {
                        span {
                            class: "badge",
                            style: "background: var(--accent); color: var(--bg); border: 1px solid var(--accent); font-weight: 700; font-size: var(--text-2xs); padding: 2px 8px;",
                            title: "Default option in the resume banner",
                            "★ Priority"
                        }
                    }
                }
                div {
                    style: "display: flex; align-items: center; gap: 6px; flex-shrink: 0;",
                    if props.agent.is_claude {
                        button {
                            class: "btn-ghost btn-sm",
                            style: "font-weight: 500; padding: 4px 10px;",
                            onclick: move |_| {
                                let mut ag = ui_state.read().custom_agents.clone();
                                let target_id = id.clone();
                                for a in &mut ag {
                                    if a.id == target_id { a.priority = !a.priority; }
                                    else if a.priority { a.priority = false; }
                                }
                                let agc = ag.clone();
                                ui_state.write().custom_agents = ag;
                                wasm_bindgen_futures::spawn_local(async move {
                                    if let Ok(json) = serde_json::to_string(&agc) {
                                        let _ = crate::tauri_bridge::store_set("custom_agents", &json).await;
                                    }
                                });
                            },
                            if props.agent.priority {
                                "Remove Priority"
                            } else {
                                "Make Priority"
                            }
                        }
                    }
                    button {
                        class: "btn-ghost btn-sm",
                        style: "font-weight: 500; padding: 4px 10px; color: var(--error);",
                        onclick: move |_| {
                            let mut ag = ui_state.read().custom_agents.clone();
                            ag.retain(|a| a.id != agent_id_for_delete);
                            let agc = ag.clone();
                            ui_state.write().custom_agents = ag;
                            wasm_bindgen_futures::spawn_local(async move {
                                if let Ok(json) = serde_json::to_string(&agc) {
                                    let _ = crate::tauri_bridge::store_set("custom_agents", &json).await;
                                }
                            });
                        },
                        "Delete"
                    }
                }
            }
            div {
                style: "font-size: var(--text-xs); color: var(--textDim); font-family: var(--fontFamily); background: var(--bgTertiary); padding: 6px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                "{cmd}"
            }
        }
    }
}

/* =============================================================
Tab: About
============================================================= */

#[component]
fn AboutSettings() -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 40px 20px; color: var(--textDim); max-width: 620px;",

            div {
                style: "font-family: var(--font-display); font-size: 36px; font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                "Athena"
            }
            div {
                style: "font-size: var(--text-sm); color: var(--textMuted); margin-top: 4px;",
                "v0.1.0"
            }
            div {
                style: "font-size: var(--text-xs); color: var(--textDim); margin-top: 8px; text-align: center; line-height: 1.6; max-width: 400px;",
                "AI-powered software orchestration and development environment. Built with Tauri, Dioxus, and a lot of coffee."
            }
        }
    }
}

/* =============================================================
Shared primitives
============================================================ */

#[derive(Props, Clone, PartialEq)]
struct SectionHeaderProps {
    title: String,
    desc: String,
}

#[component]
fn SectionHeader(props: SectionHeaderProps) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 8px; padding-bottom: 12px;",
            div {
                style: "display: flex; align-items: center; gap: 8px;",
                div {
                    style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "{props.title}"
                }
            }
            div {
                style: "font-size: var(--text-xs); color: var(--textDim); margin-top: 4px; line-height: 1.5; padding-left: 18px;",
                "{props.desc}"
            }
            hr { class: "great-circle-rule", style: "margin-top: 8px;" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SettingsSectionProps {
    label: String,
    description: Option<String>,
}

#[component]
fn SettingsSection(props: SettingsSectionProps) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 2px;",
            div {
                style: "display: flex; align-items: center; gap: 6px;",
                div {
                    style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "{props.label}"
                }
            }
            if let Some(desc) = &props.description {
                div {
                    style: "font-size: var(--text-xs); color: var(--textDim); line-height: 1.5; padding-left: 14px;",
                    "{desc}"
                }
            }
        }
    }
}

/* =============================================================
Codex of Settings — shared presentation primitives
============================================================= */

#[derive(Props, Clone, PartialEq)]
struct CodexSectionProps {
    /// Roman numeral shown in the section header, e.g. "I", "II".
    numeral: &'static str,
    /// Title text shown next to the numeral in --text + --font-display.
    title: &'static str,
    /// Short italic Cormorant line aligned to the right of the header.
    epigraph: &'static str,
    /// Optional intro line under the rule (--textMuted, --text-base).
    intro: Option<&'static str>,
    /// DOM id used by the floating index to scroll/jump-active. e.g. "s-i".
    id: &'static str,
    children: Element,
}

#[component]
fn CodexSection(props: CodexSectionProps) -> Element {
    rsx! {
        section {
            class: "codex-section",
            id: "{props.id}",
            div {
                class: "codex-section-head",
                span { class: "codex-section-num", "{props.numeral}." }
                span { class: "codex-section-title", "{props.title}" }
                span { class: "codex-section-epi", "{props.epigraph}" }
            }
            if let Some(intro) = props.intro {
                div { class: "codex-section-intro", "{intro}" }
            } else {
                div { class: "codex-section-intro" } /* keeps spacing rhythm */
            }
            hr { class: "codex-rule" }
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct GroupLabelProps {
    label: &'static str,
    /// When true, suppresses the top margin (first group in a section).
    #[props(default)]
    first: bool,
}

#[component]
pub fn GroupLabel(props: GroupLabelProps) -> Element {
    let cls = if props.first { "group-label label-first" } else { "group-label" };
    rsx! {
        div { class: "{cls}",
            span { "{props.label}" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct LabeledFieldProps {
    label: &'static str,
    description: Option<&'static str>,
    children: Element,
}

#[component]
fn LabeledField(props: LabeledFieldProps) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 4px; margin-bottom: 14px;",
            div {
                style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent);",
                "{props.label}"
            }
            if let Some(desc) = props.description {
                div {
                    style: "color: var(--textDim); font-size: 11px; padding-left: 12px;",
                    "{desc}"
                }
            }
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ToggleProps {
    active: bool,
    on_toggle: EventHandler<MouseEvent>,
}

#[component]
fn Toggle(props: ToggleProps) -> Element {
    let cls = if props.active { "toggle is-active" } else { "toggle" };
    rsx! {
        button {
            class: "{cls}",
            r#type: "button",
            aria_pressed: "{props.active}",
            onclick: move |e| props.on_toggle.call(e),
            div { class: "knob" }
        }
    }
}

/// Font-family dropdown popover. Each option is rendered in its own
/// typeface (`font-family` per option) so the user previews the face
/// they are about to pick. State (open/closed) is local to the dropdown.
///
/// The popover defaults closed and is local to this component. Selection
/// is one-way from child → parent via `on_select`.
#[derive(Props, Clone, PartialEq)]
struct FontDropdownProps {
    /// Current selection (the option rendered as the active affordance).
    current: String,
    /// Called with the chosen family name when the user picks one.
    on_select: EventHandler<String>,
}

#[component]
fn FontDropdown(props: FontDropdownProps) -> Element {
    // Local signal: is the popover open?
    let mut open = use_signal(|| false);

    // Local signal: the option count, used as the loop range. We keep it
    // as a `Vec<&'static str>` mirroring AVAILABLE_FONTS but capture it
    // inside the component as a local constant Vec — this avoids re-creating
    // the list on every render (use_signal(|| …) for closures is fine; this
    // is set once and never mutated).
    let fonts: Vec<&'static str> = AVAILABLE_FONTS.to_vec();

    rsx! {
        div {
            // Close on outside click — implemented in Task 6 via a global
            // mousedown listener; for now the popover uses an Escape or
            // a second click on the affordance to close.
            div {
                class: if open() { "font-dropdown-afford is-open" } else { "font-dropdown-afford" },
                onclick: move |_| open.toggle(),
                span { class: "name", style: "font-family: '{props.current}', monospace;", "{props.current}" }
                span { class: "chevron", "▾" }
            }
            if open() {
                div { class: "font-dropdown-pop",
                    for (idx, font) in fonts.iter().enumerate() {
                        {
                            let font_str: &'static str = font;
                            let selected = *font_str == props.current.as_str();
                            let font_for_click = font_str.to_string();
                            rsx! {
                                div {
                                    key: "{idx}",
                                    class: if selected { "font-dropdown-opt is-selected" } else { "font-dropdown-opt" },
                                    style: "font-family: '{font_str}', monospace;",
                                    onclick: move |_| {
                                        open.set(false);
                                        props.on_select.call(font_for_click.clone());
                                    },
                                    span { "{font_str}" }
                                    span { class: "check", "✓" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Font-size ± stepper. Value clamped to 10..=24 inclusive. Single click
/// only (no hold-to-repeat in v1).
#[derive(Props, Clone, PartialEq)]
struct SizeStepperProps {
    value: u8,
    on_change: EventHandler<u8>,
}

#[component]
fn SizeStepper(props: SizeStepperProps) -> Element {
    let step = |delta: i8| {
        move |_| {
            let next = (props.value as i16 + delta as i16).clamp(10, 24) as u8;
            if next != props.value {
                props.on_change.call(next);
            }
        }
    };
    rsx! {
        div { class: "size-stepper",
            button {
                class: "size-step",
                r#type: "button",
                aria_label: "Decrease font size",
                disabled: props.value <= 10,
                onclick: step(-1),
                "−"
            }
            div { class: "size-step-value",
                span { class: "px", "{props.value}" }
                span { class: "unit", "PIXELS" }
            }
            button {
                class: "size-step",
                r#type: "button",
                aria_label: "Increase font size",
                disabled: props.value >= 24,
                onclick: step(1),
                "+"
            }
        }
    }
}
