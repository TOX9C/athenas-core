use super::shortcuts_ref::ShortcutsRef;
use super::theme_picker::ThemePicker;
use crate::components::shared::icon::{
    IconAmphora, IconColumn, IconHelmet, IconScroll, IconSettings, IconTerminal,
};
use crate::components::shared::modal::Modal;
use crate::stores::athena::use_athena_store;
use crate::stores::ui::use_ui_store;
use crate::themes::{get_theme, AVAILABLE_FONTS};
use dioxus::prelude::*;

/* =============================================================
SettingsContent – shared by modal overlay and full-page panel
============================================================= */

/// Icon for a settings tab by index.
fn tab_icon(idx: u8, color: &str) -> Element {
    let c = color.to_string();
    match idx {
        0 => rsx! { IconSettings { size: Some(16), color: Some(c) } },
        1 => rsx! { IconTerminal { size: Some(16), color: Some(c) } },
        2 => rsx! { IconHelmet { size: Some(16), color: Some(c) } },
        3 => rsx! { IconColumn { size: Some(16), color: Some(c) } },
        4 => rsx! { IconScroll { size: Some(16), color: Some(c) } },
        5 => rsx! { IconAmphora { size: Some(16), color: Some(c) } },
        _ => rsx! { IconSettings { size: Some(16), color: Some(c) } },
    }
}

#[component]
pub fn SettingsContent() -> Element {
    let mut active_tab = use_signal(|| 0u8);

    let tabs = [
        ("General", 0u8),
        ("Athena", 1u8),
        ("Agents", 2u8),
        ("Themes", 3u8),
        ("Shortcuts", 4u8),
        ("About", 5u8),
    ];

    rsx! {
        div {
            style: "display: flex; height: 100%; overflow: hidden;",

            /* ── Left vertical tab bar ────────────────────── */
            div {
                style: "width: 200px; flex-shrink: 0; display: flex; flex-direction: column; gap: 4px; padding: 20px 12px; border-right: 1px solid var(--border); background: var(--bg);",

                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 0 8px 16px 8px; border-bottom: 1px solid var(--border); margin-bottom: 4px;",
                    IconSettings { size: Some(18), color: Some("var(--textDim)".to_string()) }
                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                        "Settings"
                    }
                }

                for (label, idx) in tabs {
                    {
                        let is_active = active_tab() == idx;
                        let color = if is_active { "var(--accent)" } else { "var(--textDim)" };
                        let font_weight = if is_active { "600" } else { "400" };
                        let bg = if is_active { "var(--accentSubtle)" } else { "transparent" };
                        let border = if is_active { "1px solid var(--accent)" } else { "1px solid transparent" };
                        rsx! {
                            button {
                                key: "{label}",
                                class: if is_active { "settings-tab-btn active" } else { "settings-tab-btn" },
                                style: "display: flex; align-items: center; gap: 10px; padding: 8px 12px; border: {border}; border-radius: var(--radius-md); background: {bg}; color: {color}; cursor: pointer; font-size: var(--text-sm); text-align: left; width: 100%; font-weight: {font_weight}; transition: background 0.18s ease, color 0.18s ease, border-color 0.18s ease;",
                                onclick: move |_| active_tab.set(idx),
                                {tab_icon(idx, color)}
                                "{label}"
                            }
                        }
                    }
                }

                div { style: "flex: 1;" }
            }

            /* ── Right content area ───────────────────────── */
            div {
                style: "flex: 1; padding: 24px 32px; min-width: 0; overflow-y: auto;",

                match active_tab() {
                    0 => rsx! { GeneralSettings {} },
                    1 => rsx! { AthenaSettings {} },
                    2 => rsx! { AgentsSettings {} },
                    3 => rsx! { ThemePicker {} },
                    4 => rsx! { ShortcutsRef {} },
                    5 => rsx! { AboutSettings {} },
                    _ => rsx! { GeneralSettings {} },
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
        div {
            style: "display: flex; flex-direction: column; gap: 24px; max-width: 620px;",

            SectionHeader { title: "General", desc: "Configure your Athena environment" }

            /* ── Font Family ── */
            SettingsSection { label: "Font Family", description: Some("Choose your preferred monospace typeface for the editor and terminal. View it in the preview below.".to_string()) }
            div {
                style: "display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px;",
                {
                    let ui = ui_state.read();
                    let current_font = ui.font_family.clone();
                    let theme_colors = get_theme(ui.theme.name());
                    let theme_accent = theme_colors.accent.clone();
                    let theme_bg_tertiary = theme_colors.bg_tertiary.clone();
                    drop(ui);
                    rsx! {
                        for font in AVAILABLE_FONTS {
                            {
                                let is_selected = *font == current_font;
                                let bg = if is_selected { &theme_accent } else { &theme_bg_tertiary };
                                let fg = if is_selected { "var(--bg)" } else { "var(--textMuted)" };
                                let border = if is_selected { "var(--accent)" } else { "var(--border)" };
                                let shadow = if is_selected { "0 0 0 2px var(--accentSubtle), 0 2px 4px rgba(0,0,0,0.1)" } else { "0 1px 2px rgba(0,0,0,0.05)" };
                                let font_str = font.to_string();
                                rsx! {
                                    button {
                                        key: "{font}",
                                        class: "font-option-btn",
                                        style: "padding: 8px 16px; border-radius: var(--radius-md); border: 1px solid {border}; background: {bg}; color: {fg}; cursor: pointer; font-size: var(--text-sm); font-family: '{font}', monospace; box-shadow: {shadow}; transition: all 0.18s ease; transform: scale(1);",
                                        onmouseenter: move |_| {},
                                        onclick: move |_| {
                                            let font_clone = font_str.clone();
                                            ui_state.write().font_family = font_clone;
                                            let size = ui_state.read().font_size;
                                            crate::themes::apply_font_to_dom(&font_str, size);
                                            let f = font_str.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let _ = crate::tauri_bridge::store_set("font_family", &f).await;
                                            });
                                        },
                                        "{font}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            /* ── Font Size ── */
            SettingsSection { label: "Font Size", description: Some("Adjust the base font size used throughout the interface and terminal.".to_string()) }
            div {
                style: "display: flex; align-items: center; gap: 16px; margin-top: 12px; padding: 16px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bgSecondary);",
                input {
                    r#type: "range",
                    min: "10",
                    max: "24",
                    value: "{ui_state.read().font_size}",
                    style: "flex: 1; accent-color: var(--accent); height: 6px;",
                    oninput: move |e| {
                        if let Ok(val) = e.value().parse::<u8>() {
                            let fam = ui_state.read().font_family.clone();
                            ui_state.write().font_size = val;
                            crate::themes::apply_font_to_dom(&fam, val);
                            wasm_bindgen_futures::spawn_local(async move {
                                let _ = crate::tauri_bridge::store_set("font_size", &val.to_string()).await;
                            });
                        }
                    },
                }
                span {
                    style: "font-family: var(--fontFamily); font-size: var(--text-sm); font-weight: 600; color: var(--text); min-width: 40px; text-align: center; background: var(--bgTertiary); padding: 4px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border);",
                    "{ui_state.read().font_size}px"
                }
            }

            /* ── Preview ── */
            div {
                class: "settings-preview",
                style: "margin-top: 16px; padding: 20px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bgSecondary); position: relative; overflow: hidden;",
                div {
                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 12px; padding-bottom: 10px; border-bottom: 1px solid var(--border);",
                    div {
                        style: "width: 10px; height: 10px; border-radius: 50%; background: var(--error);",
                    }
                    div {
                        style: "width: 10px; height: 10px; border-radius: 50%; background: var(--warning);",
                    }
                    div {
                        style: "width: 10px; height: 10px; border-radius: 50%; background: var(--success);",
                    }
                    div {
                        style: "font-size: var(--text-2xs); color: var(--textDim); margin-left: auto; font-weight: 500;",
                        "Preview"
                    }
                }
                div {
                    style: "font-family: '{ui_state.read().font_family}', monospace; font-size: {ui_state.read().font_size}px; color: var(--text); line-height: 1.75;",
                    "fn main() {{"
                    br {}
                    "    println!(\"Hello, world!\");"
                    br {}
                    "}}"
                }
            }

            SectionHeader { title: "Pane Titles", desc: "Auto-generated labels above each pane" }
            /* ── Smart pane titles toggle ── */
            div {
                style: "display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 16px 20px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bgSecondary); margin-top: 12px;",
                div {
                    style: "display: flex; flex-direction: column; gap: 4px; min-width: 0; padding-right: 8px;",
                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--text);",
                        "Smart pane titles"
                    }
                    div {
                        style: "font-size: var(--text-xs); color: var(--textMuted); line-height: 1.5;",
                        "Auto-generate names for idle shells and summarize agent titles via LLM."
                    }
                }
                {
                    let enabled = ui_state.read().smart_pane_titles;
                    let bg = if enabled { "var(--accent)" } else { "var(--bgTertiary)" };
                    let knob = if enabled { "translateX(22px)" } else { "translateX(2px)" };
                    let knob_bg = if enabled { "var(--bg)" } else { "var(--textDim)" };
                    rsx! {
                        button {
                            style: "position: relative; width: 48px; height: 26px; border-radius: 999px; border: 1px solid var(--border); background: {bg}; cursor: pointer; padding: 0; flex-shrink: 0; transition: background 0.2s ease; box-shadow: inset 0 1px 2px rgba(0,0,0,0.15);",
                            onclick: move |_| {
                                let next = !ui_state.read().smart_pane_titles;
                                ui_state.write().smart_pane_titles = next;
                                wasm_bindgen_futures::spawn_local(async move {
                                    let _ = crate::tauri_bridge::store_set(
                                        "smart_pane_titles",
                                        if next { "true" } else { "false" },
                                    )
                                    .await;
                                });
                            },
                            div {
                                style: "position: absolute; top: 2px; left: 0px; width: 20px; height: 20px; border-radius: 50%; background: {knob_bg}; transform: {knob}; transition: transform 0.2s ease, background 0.2s ease; box-shadow: 0 1px 3px rgba(0,0,0,0.2);",
                            }
                        }
                    }
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

    let agents_snapshot: Vec<crate::types::workspace::CustomAgent> =
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
                        style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--text);",
                        "Custom Agents"
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
                        style: "display: flex; flex-direction: column; gap: 14px; padding: 20px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bgSecondary); box-shadow: 0 2px 8px rgba(0,0,0,0.04);",
                        div {
                            style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--text); margin-bottom: 4px;",
                            "New Agent"
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
                                    style: "font-family: var(--font-ui); font-size: var(--text-sm); font-weight: 600; color: var(--text);",
                                    "Treat as Claude"
                                }
                                span {
                                    style: "font-family: var(--font-ui); font-size: var(--text-xs); color: var(--textDim);",
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
                                        style: "font-family: var(--font-ui); font-size: var(--text-sm); font-weight: 600; color: var(--text);",
                                        "Set as Priority"
                                    }
                                    span {
                                        style: "font-family: var(--font-ui); font-size: var(--text-xs); color: var(--textDim);",
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
                    style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--text); margin-bottom: 8px;",
                    "Built-in Agents"
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
                style: "font-size: var(--text-xs); color: var(--textDim); font-family: var(--fontFamily); background: var(--bg); padding: 6px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
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
            style: "margin-bottom: 8px; padding-bottom: 12px; border-bottom: 1px solid var(--border);",
            div {
                style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                "{props.title}"
            }
            div {
                style: "font-size: var(--text-xs); color: var(--textDim); margin-top: 4px; line-height: 1.5;",
                "{props.desc}"
            }
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
                style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color lattice: var(--text);",
                "{props.label}"
            }
            if let Some(desc) = &props.description {
                div {
                    style: "font-size: var(--text-xs); color: var(--textDim); line-height: 1.5;",
                    "{desc}"
                }
            }
        }
    }
}

// Custom toggle switch for consistent, modern UI
#[derive(Props, Clone, PartialEq)]
struct CustomToggleProps {
    active: bool,
}

#[component]
fn CustomToggle(props: CustomToggleProps) -> Element {
    let bg = if props.active { "var(--accent)" } else { "var(--bgTertiary)" };
    let knob = if props.active { "translateX(22px)" } else { "translateX(2px)" };
    let knob_bg = if props.active { "var(--bg)" } else { "var(--textDim)" };
    rsx! {
        div {
            style: "flex-shrink: 0; width: 48px; height: 26px; border-radius: 999px; background: {bg}; border: 1px solid var(--border); position: relative; box-shadow: inset 0 1px 2px rgba(0,0,0,0.12); transition: background 0.18s ease, border-color 0.18s ease; pointer-events: none;",
            div {
                style: "position: absolute; top: 2px; left: 0px; width: 20px; height: 20px; border-radius: 50%; background: {knob_bg}; transform: {knob}; box-shadow: 0 1px 3px rgba(0,0,0,0.25); transition: transform 0.18s cubic-bezier(0.4, 0, 0.2, 1), background 0.18s ease; will-change: transform;",
            }
        }
    }
}
