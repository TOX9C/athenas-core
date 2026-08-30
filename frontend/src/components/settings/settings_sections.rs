//! Settings section components for General, Athena, and About.

use super::{FontDropdown, GroupLabel, LabeledField, SizeStepper, Toggle};
use crate::components::settings::provider_presets::{
    infer_provider_id, provider_preset, LLM_PROVIDERS,
};
use crate::components::shared::icon::{IconCheck, IconClose};
use crate::stores::athena::use_athena_store;
use crate::stores::ui::use_ui_store;
use crate::utils::font_size::persist_font_size;
use dioxus::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/* =============================================================
Tab: General
============================================================= */

#[component]
pub(super) fn GeneralSettings() -> Element {
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
                    persist_font_size(val);
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

/// Fetch the model list for the current base URL + key. The typed key is
/// passed through so "Fetch models" works before the user hits Save; the
/// backend falls back to the keyring slot for `provider` when it is empty.
///
/// Kept as a free function (signals are `Copy` in Dioxus 0.7) so it can be
/// called from event handlers *and* from the async load/save flows.
#[allow(clippy::too_many_arguments)]
fn fetch_models(
    base_url: Signal<String>,
    api_key_input: Signal<String>,
    provider: Signal<String>,
    mut models_loading: Signal<bool>,
    mut models_error: Signal<String>,
    mut available_models: Signal<Vec<String>>,
    mut models_fetched_for_url: Signal<String>,
) {
    let url = base_url.read().clone();
    if url.trim().is_empty() {
        return;
    }
    let key = api_key_input.read().clone();
    let prov = provider.read().clone();
    models_loading.set(true);
    models_error.set(String::new());
    available_models.set(Vec::new());
    wasm_bindgen_futures::spawn_local(async move {
        match crate::tauri_bridge::llm_list_models(&url, &key, &prov).await {
            Ok(json) => {
                let parsed: serde_json::Value = match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(_) => {
                        models_error.set("Could not parse the models response".to_string());
                        models_loading.set(false);
                        return;
                    }
                };
                let ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                if ok {
                    let models: Vec<String> = parsed
                        .get("models")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|m| m.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    available_models.set(models);
                    models_fetched_for_url.set(url);
                } else {
                    let msg = parsed
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown response")
                        .to_string();
                    models_error.set(msg);
                }
            }
            Err(e) => {
                models_error.set(format!("{:?}", e));
            }
        }
        models_loading.set(false);
    });
}

/// Load the persisted config for a provider into the settings signals.
///
/// `legacy` reads the legacy/custom slots (`llm.base_url`, `llm.model`,
/// `llm.api_key`) instead of the provider-scoped ones — used on first mount
/// for existing users and whenever the Custom preset is selected. When
/// `auto_fetch` is set and the provider exposes a `/models` list with a key
/// available, the model list is fetched immediately so the user gets a
/// dropdown without clicking "Fetch models".
#[allow(clippy::too_many_arguments)]
fn load_provider_config(
    prov: String,
    legacy: bool,
    auto_fetch: bool,
    mut provider: Signal<String>,
    mut base_url: Signal<String>,
    mut model: Signal<String>,
    mut api_key_set: Signal<bool>,
    api_key_input: Signal<String>,
    mut available_models: Signal<Vec<String>>,
    mut models_fetched_for_url: Signal<String>,
    mut models_error: Signal<String>,
    mut save_status: Signal<Option<bool>>,
    models_loading: Signal<bool>,
) {
    provider.set(prov.clone());
    save_status.set(None);
    // A provider switch invalidates any fetched model list.
    available_models.set(Vec::new());
    models_fetched_for_url.set(String::new());
    models_error.set(String::new());

    let url_key = if legacy {
        "llm.base_url".to_string()
    } else {
        format!("llm.base_url.{prov}")
    };
    let model_key = if legacy {
        "llm.model".to_string()
    } else {
        format!("llm.model.{prov}")
    };
    let api_key_key = if legacy {
        "llm.api_key".to_string()
    } else {
        format!("llm.api_key.{prov}")
    };

    wasm_bindgen_futures::spawn_local(async move {
        // Base URL: the saved value wins, else the preset default.
        match crate::tauri_bridge::store_get(&url_key).await {
            Ok(url) if !url.trim().is_empty() => base_url.set(url),
            _ => base_url.set(
                provider_preset(&prov)
                    .map(|p| p.default_base_url.to_string())
                    .unwrap_or_default(),
            ),
        }
        // Model: saved value, else the preset default (e.g. GLM 5.2 on NIM)
        // so the field is never blank for a preset that ships one.
        match crate::tauri_bridge::store_get(&model_key).await {
            Ok(m) if !m.trim().is_empty() => model.set(m),
            _ => model.set(
                provider_preset(&prov)
                    .and_then(|p| p.default_model)
                    .map(str::to_string)
                    .unwrap_or_default(),
            ),
        }
        // Key status: scoped slot for presets, legacy slot for custom.
        let status = crate::tauri_bridge::store_get(&api_key_key)
            .await
            .unwrap_or_default();
        api_key_set.set(status == "set");

        if auto_fetch
            && provider_preset(&prov).is_some_and(|p| p.supports_model_list)
            && (api_key_set() || !api_key_input.read().trim().is_empty())
        {
            fetch_models(
                base_url,
                api_key_input,
                provider,
                models_loading,
                models_error,
                available_models,
                models_fetched_for_url,
            );
        }
    });
}

#[component]
pub(super) fn AthenaSettings() -> Element {
    let mut api_key_set = use_signal(|| false);
    let mut api_key_input = use_signal(String::new);
    let mut base_url = use_signal(|| "https://api.openai.com/v1".to_string());
    let mut model = use_signal(|| "gpt-4o".to_string());
    let mut save_status = use_signal(|| Option::<bool>::None);
    let mut save_error = use_signal(String::new);
    let mut athena_state = use_athena_store();
    let mut toast_store = crate::components::shared::toast::use_toast_store();

    // Provider preset + fetched model list. `provider` holds the selected
    // preset id (mirrors the persisted `llm.provider` value).
    let provider = use_signal(String::new);
    let mut available_models = use_signal(Vec::<String>::new);
    let models_loading = use_signal(|| false);
    let mut models_error = use_signal(String::new);
    // Base URL the fetched model list came from — editing the URL invalidates it.
    let mut models_fetched_for_url = use_signal(String::new);

    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            // Resolve the persisted provider: an explicit `llm.provider` preset
            // id wins; otherwise (existing users / custom) infer from the
            // legacy base URL so the dropdown lands on the right preset.
            let persisted = crate::tauri_bridge::store_get("llm.provider")
                .await
                .unwrap_or_default();
            let persisted = persisted.trim().to_string();
            let has_persisted = provider_preset(&persisted).is_some();
            let prov = if has_persisted {
                persisted.clone()
            } else {
                match crate::tauri_bridge::store_get("llm.base_url").await {
                    Ok(url) if !url.trim().is_empty() => infer_provider_id(&url).to_string(),
                    _ => "custom".to_string(),
                }
            };
            load_provider_config(
                prov,
                // Legacy slots only for existing users / custom; providers
                // with a persisted id read their own scoped keys.
                !has_persisted,
                true,
                provider,
                base_url,
                model,
                api_key_set,
                api_key_input,
                available_models,
                models_fetched_for_url,
                models_error,
                save_status,
                models_loading,
            );
        });
    });

    let mut do_save = move || {
        let new_key = api_key_input.read().clone();
        let url = base_url.read().clone();
        let m = model.read().clone();
        let prov = provider.read().clone();
        // Custom (or nothing picked yet) stores to the legacy slots; presets
        // store to their own scoped slots so each provider keeps its own key,
        // model, and base URL.
        let is_custom = prov == "custom" || prov.trim().is_empty();
        let api_key_key = if is_custom {
            "llm.api_key".to_string()
        } else {
            format!("llm.api_key.{prov}")
        };
        let url_key = if is_custom {
            "llm.base_url".to_string()
        } else {
            format!("llm.base_url.{prov}")
        };
        let model_key = if is_custom {
            "llm.model".to_string()
        } else {
            format!("llm.model.{prov}")
        };
        let key_will_be_set = !new_key.is_empty() || api_key_set();
        api_key_input.set(String::new());

        wasm_bindgen_futures::spawn_local(async move {
            let mut key_ok = key_will_be_set;
            let mut key_err = String::new();

            if !new_key.is_empty() {
                match crate::tauri_bridge::store_set(&api_key_key, &new_key).await {
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
            if let Err(e) = crate::tauri_bridge::store_set(&url_key, &url).await {
                key_err = if key_err.is_empty() {
                    format!("base URL: {:?}", e)
                } else {
                    format!("{}; base URL: {:?}", key_err, e)
                };
            }
            if let Err(e) = crate::tauri_bridge::store_set(&model_key, &m).await {
                key_err = if key_err.is_empty() {
                    format!("model: {:?}", e)
                } else {
                    format!("{}; model: {:?}", key_err, e)
                };
            }
            // Provider: presets persist their id so the backend routes to
            // their scoped keys; Custom deletes it so host-based inference
            // stays active (e.g. a localhost URL keeps LM Studio's no-vision
            // flag) and the legacy slots remain authoritative.
            let prov_result = if is_custom {
                crate::tauri_bridge::store_delete("llm.provider").await
            } else {
                crate::tauri_bridge::store_set("llm.provider", &prov).await
            };
            if let Err(e) = prov_result {
                key_err = if key_err.is_empty() {
                    format!("provider: {:?}", e)
                } else {
                    format!("{}; provider: {:?}", key_err, e)
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

            // The key is now persisted (or confirmed already-set) — pull the
            // model list so the user can pick from a dropdown right away.
            if !any_error && key_ok && provider_preset(&prov).is_some_and(|p| p.supports_model_list)
            {
                fetch_models(
                    base_url,
                    api_key_input,
                    provider,
                    models_loading,
                    models_error,
                    available_models,
                    models_fetched_for_url,
                );
            }
        });
    };

    // Whether the current provider exposes an OpenAI-compatible /models list.
    // Custom is OpenAI-compatible, so it counts; only Anthropic is excluded.
    let current_supports_models =
        provider_preset(&provider()).is_some_and(|p| p.supports_model_list);
    // Whether the fetched list is still valid for the URL currently in the field.
    let models_are_current = !available_models.read().is_empty()
        && models_fetched_for_url.read().as_str() == base_url.read().as_str();

    rsx! {
        /* GroupLabel "Provider" sits first to anchor the first labeled field. */
        GroupLabel { label: "Provider", first: true }

        LabeledField {
            label: "Provider",
            description: Some("Each provider keeps its own API key, base URL, and model. Presets prefill the base URL; Custom accepts any OpenAI-compatible endpoint."),
            select {
                value: "{provider}",
                class: "field",
                style: "width: 100%; box-sizing: border-box;",
                onchange: move |e| {
                    let id = e.value();
                    // The typed key belongs to the previous provider — clear it
                    // so it isn't saved to the newly selected one.
                    api_key_input.set(String::new());
                    save_error.set(String::new());
                    load_provider_config(
                        id.clone(),
                        // Custom reads the legacy slots (it has no scoped keys).
                        id == "custom",
                        // Auto-fetch /models when a key is already saved for it.
                        true,
                        provider,
                        base_url,
                        model,
                        api_key_set,
                        api_key_input,
                        available_models,
                        models_fetched_for_url,
                        models_error,
                        save_status,
                        models_loading,
                    );
                },
                for preset in LLM_PROVIDERS {
                    option { value: "{preset.id}", "{preset.label}" }
                }
            }
        }

        LabeledField {
            label: "API Key",
            description: Some("Stored per provider in the OS keychain. Paste to replace."),
            div {
                style: "display: flex; align-items: center; gap: 8px; margin-top: 4px;",
                span {
                    style: "font-size: 10px; color: var(--textMuted); background: var(--bgTertiary); padding: 4px 9px; border-radius: var(--radius-sm); border: 1px solid var(--border); font-family: var(--font-ui);",
                    if api_key_set() { "●●●● Set" } else { "Not set" }
                }
                button {
                    class: "btn-secondary btn-sm",
                    style: "padding: 4px 10px; font-weight: 500;",
                    title: "Test keyring access",
                    onclick: move |_| {
                        let mut toast = toast_store;
                        wasm_bindgen_futures::spawn_local(async move {
                            match crate::tauri_bridge::test_llm_api_key().await {
                                Ok(json) => {
                                    let parsed: serde_json::Value = match serde_json::from_str(&json) {
                                        Ok(v) => v,
                                        Err(_) => {
                                            toast.write().push(crate::components::shared::toast::Toast {
                                                id: format!("test-key-{}", chrono::Utc::now().timestamp_millis()),
                                                toast_type: crate::components::shared::toast::ToastType::Warning,
                                                title: "Key test error".to_string(),
                                                message: "Could not parse key test response".to_string(),
                                                duration_ms: 4000,
                                            });
                                            return;
                                        }
                                    };
                                    let ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let msg = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown response");
                                    toast.write().push(crate::components::shared::toast::Toast {
                                        id: format!("test-key-{}", chrono::Utc::now().timestamp_millis()),
                                        toast_type: if ok {
                                            crate::components::shared::toast::ToastType::Success
                                        } else {
                                            crate::components::shared::toast::ToastType::Warning
                                        },
                                        title: if ok { "Key OK".to_string() } else { "Key test failed".to_string() },
                                        message: msg.to_string(),
                                        duration_ms: 6000,
                                    });
                                }
                                Err(e) => {
                                    toast.write().push(crate::components::shared::toast::Toast {
                                        id: format!("test-key-{}", chrono::Utc::now().timestamp_millis()),
                                        toast_type: crate::components::shared::toast::ToastType::Error,
                                        title: "Key test failed".to_string(),
                                        message: format!("{:?}", e),
                                        duration_ms: 5000,
                                    });
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

        LabeledField {
            label: "Base URL",
            description: Some("e.g. https://api.openai.com/v1, https://api.groq.com/openai/v1, http://localhost:1234/v1"),
            input {
                value: "{base_url}",
                class: "field",
                style: "width: 100%; box-sizing: border-box;",
                placeholder: "https://api.openai.com/v1",
                readonly: provider() == "anthropic",
                oninput: move |e| {
                    base_url.set(e.value());
                    // Editing the URL invalidates the fetched model list.
                    available_models.set(Vec::new());
                    models_fetched_for_url.set(String::new());
                    models_error.set(String::new());
                    save_status.set(None);
                },
            }
            if provider() == "anthropic" {
                div { class: "settings-provider-hint", "Anthropic uses a fixed endpoint; custom Anthropic URLs aren't supported yet." }
            }
        }

        LabeledField {
            label: "Model",
            description: Some("gpt-4o, claude-sonnet-4-6, llama3.1, …"),
            div {
                class: "settings-inline-row",
                style: "width: 100%;",
                input {
                    value: "{model}",
                    class: "field",
                    style: "flex: 1; min-width: 0; box-sizing: border-box;",
                    placeholder: "gpt-4o, gpt-4, llama3.1, …",
                    oninput: move |e| { model.set(e.value()); save_status.set(None); },
                }
                if current_supports_models {
                    button {
                        class: "btn-secondary btn-sm settings-fetch-models",
                        r#type: "button",
                        disabled: models_loading(),
                        title: "Fetch available models from {base_url}/models",
                        onclick: move |_| {
                            fetch_models(
                                base_url,
                                api_key_input,
                                provider,
                                models_loading,
                                models_error,
                                available_models,
                                models_fetched_for_url,
                            );
                        },
                        if models_loading() { "Fetching…" } else { "Fetch models" }
                    }
                }
            }
            if !models_error.read().is_empty() {
                div { style: "font-size: 11px; color: var(--error); padding-left: 12px; margin-top: 4px;", "{models_error.read()}" }
            }
            if models_are_current && !available_models.read().is_empty() {
                select {
                    class: "field",
                    style: "width: 100%; box-sizing: border-box; margin-top: 8px;",
                    onchange: move |e| { model.set(e.value()); save_status.set(None); },
                    option { value: "", "Select a model…" }
                    for m in available_models.read().iter() {
                        option { value: "{m}", "{m}" }
                    }
                }
            }
            // Preset-specific model guidance (e.g. the GLM 5.2 reasoning note
            // on NVIDIA NIM).
            if let Some(hint) = provider_preset(&provider()).and_then(|p| p.model_hint) {
                div { class: "settings-provider-hint", style: "margin-top: 6px;", "{hint}" }
            }
        }

        /* Save row */
        div {
            style: "display: flex; align-items: center; gap: 12px; margin-top: 10px; padding-top: 14px; border-top: 1px solid var(--border);",
            button {
                class: "save-btn",
                r#type: "button",
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
                        style: "display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--success); font-weight: 500;",
                        IconCheck { size: Some(13), color: Some("var(--success)".to_string()) }
                        "Saved"
                    }
                },
                Some(false) => rsx! {
                    span {
                        style: "display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--error); font-weight: 500;",
                        IconClose { size: Some(12), color: Some("var(--error)".to_string()) }
                        "Save failed — see notification"
                    }
                },
                None => rsx! {},
            }
        }
    }
}

/* =============================================================
Tab: About
============================================================= */

fn browser_diagnostic_value(object_name: &str, method_name: &str) -> String {
    let Some(window) = web_sys::window() else {
        return String::new();
    };
    let Ok(object) = js_sys::Reflect::get(&window, &JsValue::from_str(object_name)) else {
        return String::new();
    };
    let Ok(method) = js_sys::Reflect::get(&object, &JsValue::from_str(method_name)) else {
        return String::new();
    };
    let Ok(method) = method.dyn_into::<js_sys::Function>() else {
        return String::new();
    };
    method
        .call0(&object)
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

#[component]
pub(super) fn AboutSettings() -> Element {
    let mut exporting = use_signal(|| false);
    let mut export_status = use_signal(String::new);
    let export_diagnostics = move |_| {
        if exporting() {
            return;
        }
        exporting.set(true);
        export_status.set("Collecting redacted diagnostics…".to_string());
        let frontend_logs = browser_diagnostic_value("__athenaDiagnostics", "getConsole");
        let frontend_metrics = browser_diagnostic_value("__athenaDiagnostics", "getMetrics");
        let mut exporting = exporting;
        let mut export_status = export_status;
        wasm_bindgen_futures::spawn_local(async move {
            match crate::tauri_bridge::diagnostics_export(&frontend_logs, &frontend_metrics).await {
                Ok(path) => {
                    export_status.set(format!("Saved redacted bundle: {path}"));
                }
                Err(error) => {
                    export_status.set(format!("Diagnostics export failed: {error:?}"));
                }
            }
            exporting.set(false);
        });
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; padding: 36px 20px 20px; color: var(--textDim); max-width: 620px; margin: 0 auto;",
            span {
                style: "width: 54px; height: 54px; border-radius: 50%; border: 1px solid var(--accent); display: inline-flex; align-items: center; justify-content: center; color: var(--accent); font-family: var(--font-display); font-size: 32px; box-shadow: 0 0 18px var(--accentSubtle), inset 0 0 10px var(--accentSubtle); margin-bottom: 16px;",
                "\u{0398}"
            }
            div {
                style: "font-family: var(--font-display); color: var(--text); font-size: 40px; font-weight: 600; letter-spacing: 0.01em; line-height: 1;",
                "Athena"
            }
            div {
                style: "font-family: var(--font-display); color: var(--accent); font-size: 13px; letter-spacing: 0.14em; text-transform: uppercase; margin-top: 6px;",
                "v 0.3.0"
            }
            div {
                style: "font-family: var(--font-display); font-style: italic; color: var(--textDim); font-size: 13px; margin-top: 14px; max-width: 380px; text-align: center; line-height: 1.6;",
                "AI-powered software orchestration and development environment. Built with Tauri, Dioxus, and a lot of coffee."
            }
        }

        // ── Diagnostics ──
        div {
            style: "max-width: 620px; margin: 0 auto; padding: 0 20px 28px;",
            div {
                style: "border-top: 1px solid var(--border); padding-top: 24px; margin-top: 8px;",
                div {
                    style: "font-family: var(--font-display); font-size: 15px; font-weight: 600; color: var(--accent); margin-bottom: 6px;",
                    "Diagnostics"
                }
                p {
                    style: "font-family: var(--font-display); font-style: italic; color: var(--textDim); font-size: 12px; line-height: 1.6; margin-bottom: 14px;",
                    "Export a bounded, redacted bundle of runtime errors, warnings, performance metrics, and backend logs for troubleshooting. API keys, authorization headers, prompt content, and private home paths are excluded where detected."
                }
                button {
                    class: "btn-secondary btn-sm",
                    r#type: "button",
                    disabled: exporting(),
                    onclick: export_diagnostics,
                    if exporting() { "Exporting…" } else { "Export diagnostics" }
                }
                if !export_status().is_empty() {
                    div {
                        style: "margin-top: 10px; color: var(--textDim); font-family: var(--font-mono); font-size: 10px; word-break: break-word;",
                        "{export_status}"
                    }
                }
            }
        }

        // ── Support the Developer ──
        div {
            style: "max-width: 620px; margin: 0 auto; padding: 0 20px 36px;",
            div {
                style: "border-top: 1px solid var(--border); padding-top: 28px; margin-top: 8px;",
                div {
                    style: "font-family: var(--font-display); font-size: 15px; font-weight: 600; color: var(--accent); margin-bottom: 6px;",
                    "Support the Developer"
                }
                p {
                    style: "font-family: var(--font-display); font-style: italic; color: var(--textDim); font-size: 12px; line-height: 1.6; margin-bottom: 20px;",
                    "Athena\'s Core is free and open source (MIT). If it saves you time, donations are appreciated."
                }
                // Crypto addresses
                div {
                    style: "display: flex; flex-direction: column; gap: 10px; margin-bottom: 16px;",
                    div {
                        style: "display: flex; flex-direction: column; gap: 3px;",
                        span { style: "font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent);", "BTC" }
                        code { style: "font-family: var(--font-mono); font-size: 11px; color: var(--textDim); background: var(--bgSecondary); border: 1px solid var(--border); padding: 8px 10px; border-radius: 4px; word-break: break-all; display: block;", "bc1qn8ehwc7rxlpgvljztr5k6npqf307xq00dqatf8" }
                    }
                    div {
                        style: "display: flex; flex-direction: column; gap: 3px;",
                        span { style: "font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent);", "ETH / USDT / USDC (ERC-20)" }
                        code { style: "font-family: var(--font-mono); font-size: 11px; color: var(--textDim); background: var(--bgSecondary); border: 1px solid var(--border); padding: 8px 10px; border-radius: 4px; word-break: break-all; display: block;", "0x4260456e1dbdc880d69d75949726953215a93586" }
                    }
                    div {
                        style: "display: flex; flex-direction: column; gap: 3px;",
                        span { style: "font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent);", "USDT (TRC-20)" }
                        code { style: "font-family: var(--font-mono); font-size: 11px; color: var(--textDim); background: var(--bgSecondary); border: 1px solid var(--border); padding: 8px 10px; border-radius: 4px; word-break: break-all; display: block;", "TSBUpAreTjmUscbUbf4L1wkX1fvvJvSRGW" }
                    }
                }
                // Links
                div {
                    style: "display: flex; gap: 12px; flex-wrap: wrap;",
                    a {
                        href: "https://tox9c.github.io/athenas-core/#support",
                        target: "_blank",
                        style: "font-family: var(--font-mono); font-size: 11px; color: var(--accent); border: 1px solid var(--border); border-radius: 4px; padding: 6px 12px; text-decoration: none;",
                        "Donate online \u{2192}"
                    }
                    a {
                        href: "https://github.com/TOX9C/athenas-core",
                        target: "_blank",
                        style: "font-family: var(--font-mono); font-size: 11px; color: var(--textDim); border: 1px solid var(--border); border-radius: 4px; padding: 6px 12px; text-decoration: none;",
                        "Star on GitHub \u{2192}"
                    }
                }
            }
        }
    }
}

/* =============================================================
Tab: Mobile Mirror (relay)
============================================================= */

/// Local mirror of the `RelayStatus` shape returned by `relay_status`.
/// Kept here (not in `tauri_bridge::types`) because it is parsed from the
/// JSON string the wrapper returns — following the `session_list` /
/// `output_buffer_list` string-return convention.
#[derive(Debug, Clone, serde::Deserialize, Default)]
struct RelayStatus {
    running: bool,
    url: Option<String>,
    #[serde(default)]
    qr_svg_base64: Option<String>,
}

async fn copy_to_clipboard(text: String) -> Result<(), ()> {
    let window = web_sys::window().ok_or(())?;
    let navigator =
        js_sys::Reflect::get(&window, &JsValue::from_str("navigator")).map_err(|_| ())?;
    let clipboard =
        js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")).map_err(|_| ())?;
    let write_text = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))
        .map_err(|_| ())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| ())?;
    let promise = write_text
        .call1(&clipboard, &JsValue::from_str(&text))
        .map_err(|_| ())?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| ())?;
    JsFuture::from(promise).await.map_err(|_| ())?;
    Ok(())
}

#[component]
pub(super) fn MobileMirrorSettings() -> Element {
    /// Translate raw bridge/wasm error strings into user-facing copy. Internal
    /// plumbing failures (Tauri bridge missing, wasm glue errors, stack-frame
    /// dumps) collapse to a friendly one-liner; the raw text is preserved for a
    /// "Technical details" fold so debugging information is never lost.
    fn relay_error_message(raw: &str) -> (String, Option<String>) {
        let lower = raw.to_ascii_lowercase();
        let internal = [
            "__tauri__",
            "reflect",
            "__wbindgen",
            "dispatch error",
            "wasm",
            "js bridge",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
        if internal {
            (
                "The local relay is unavailable from this interface.".to_string(),
                Some(raw.to_string()),
            )
        } else {
            (raw.to_string(), None)
        }
    }

    // Live running state from `relay_status` (authoritative — the persisted
    // `relay.enabled` flag could be stale if the relay failed to bind).
    let mut running = use_signal(|| false);
    let mut url = use_signal(String::new);
    let mut qr_svg_base64 = use_signal(String::new);
    let mut copied = use_signal(|| false);
    // In-flight toggle so the user gets feedback while start/stop resolves.
    let mut pending = use_signal(|| false);
    // Last async error, surfaced briefly under the toggle.
    let mut last_error = use_signal(String::new);

    // Pull live status on mount.
    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            match crate::tauri_bridge::relay_status().await {
                Ok(json) => {
                    if let Ok(s) = serde_json::from_str::<RelayStatus>(&json) {
                        running.set(s.running);
                        url.set(s.url.unwrap_or_default());
                        qr_svg_base64.set(s.qr_svg_base64.unwrap_or_default());
                    }
                }
                Err(e) => {
                    if let Some(msg) = e.as_string() {
                        last_error.set(msg);
                    }
                }
            }
        });
    });

    // Pre-compute the status-pill color strings — rsx! format interpolation
    // chokes on nested ternaries with escaped quotes inline, so we lift them
    // out to keep the `style:` attribute readable.
    let dot_bg = if *running.read() {
        "var(--success)"
    } else {
        "var(--textDim)"
    };
    let dot_shadow = if *running.read() {
        "var(--success)"
    } else {
        "transparent"
    };

    rsx! {
        GroupLabel { label: "Mobile Mirror", first: true }

        p {
            style: "font-family: var(--font-display); font-style: italic; color: var(--textDim); font-size: 12px; line-height: 1.55; margin: 0 0 16px; max-width: 560px;",
            "Serve this desktop interface over your local network. The QR/link includes a private session token; only share it with devices you trust."
        }

        /* ── Toggle row ── */
        div {
            style: "display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 14px 16px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); margin-bottom: 8px;",
            div {
                style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
                span {
                    style: "font-family: var(--font-display); font-size: 13px; font-weight: 600; color: var(--accent);",
                    "Enable mobile mirror"
                }
                span {
                    style: "font-size: 11px; color: var(--textDim);",
                    "Bind a local port and accept authenticated connections from devices on your LAN."
                }
                if !last_error.read().is_empty() {
                    {
                        let (msg, detail) = relay_error_message(&last_error.read());
                        rsx! {
                            span {
                                style: "font-size: 11px; color: var(--error); margin-top: 2px; word-break: break-word;",
                                "{msg}"
                            }
                            if let Some(detail) = detail {
                                details {
                                    style: "margin-top: 2px; font-size: 10px; color: var(--textDim);",
                                    summary { style: "cursor: pointer;", "Technical details" }
                                    pre {
                                        style: "white-space: pre-wrap; word-break: break-word; margin: 4px 0 0; font-family: var(--font-mono, monospace);",
                                        "{detail}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Toggle {
                active: *running.read() && !*pending.read(),
                on_toggle: move |_| {
                    if *pending.read() { return; }
                    let next = !*running.read();
                    pending.set(true);
                    last_error.set(String::new());
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = if next {
                            crate::tauri_bridge::relay_start().await.map(|_| ())
                        } else {
                            crate::tauri_bridge::relay_stop().await
                        };
                        match result {
                            Ok(_) => {
                                // Refresh live status: relay_start returns the
                                // bound address; relay_stop returns ().
                                if let Ok(json) = crate::tauri_bridge::relay_status().await {
                                    if let Ok(s) = serde_json::from_str::<RelayStatus>(&json) {
                                        running.set(s.running);
                                        url.set(s.url.unwrap_or_default());
                                        qr_svg_base64.set(s.qr_svg_base64.unwrap_or_default());
                                                    }
                                }
                            }
                            Err(e) => {
                                if let Some(msg) = e.as_string() {
                                    last_error.set(msg);
                                }
                            }
                        }
                        pending.set(false);
                    });
                }
            }
        }

        /* ── Status pill + pairing details ── */
        div {
            style: "display: flex; align-items: center; gap: 12px; padding: 12px 16px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary);",
            span {
                style: "width: 8px; height: 8px; border-radius: 50%; background: {dot_bg}; box-shadow: 0 0 6px {dot_shadow};",
            }
            span {
                style: "font-family: var(--font-display); font-size: 12px; font-weight: 600; color: var(--text); letter-spacing: 0.08em; text-transform: uppercase;",
                if *running.read() { "Running" } else { "Stopped" }
            }
            if *pending.read() {
                span {
                    style: "margin-left: auto; font-size: 11px; color: var(--textDim);",
                    "Working…"
                }
            }
        }

        if *running.read() && !url.read().is_empty() {
            div { class: "mobile-pairing-card",
                div { class: "mobile-pairing-copy",
                    div { class: "mobile-eyebrow", "PAIR THIS DESKTOP" }
                    p { "Scan the QR code with your phone, or copy the private link. Your phone must be on the same trusted Wi‑Fi network." }
                    div { class: "mobile-pairing-url", "{url.read()}" }
                    div { class: "mobile-pairing-actions",
                        button {
                            class: "btn-primary btn-sm",
                            onclick: move |_| {
                                let link = url.read().clone();
                                if !link.is_empty() {
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match copy_to_clipboard(link).await {
                                            Ok(()) => {
                                                copied.set(true);
                                                last_error.set(String::new());
                                                gloo::timers::future::TimeoutFuture::new(2200).await;
                                                copied.set(false);
                                            }
                                            Err(()) => {
                                                last_error.set("Clipboard access was unavailable; select the link to copy it manually.".to_string());
                                            }
                                        }
                                    });
                                }
                            },
                            if *copied.read() { "Copied" } else { "Copy link" }
                        }
                    }
                    if !last_error.read().is_empty() {
                        span { class: "mobile-pairing-error", "{last_error.read()}" }
                    }
                }
                if !qr_svg_base64.read().is_empty() {
                    img {
                        class: "mobile-pairing-qr",
                        src: "data:image/svg+xml;base64,{qr_svg_base64.read()}",
                        alt: "QR code for the Athena mobile companion link",
                    }
                }
            }
            p { class: "mobile-pairing-warning", "The link grants access to this desktop while Mobile Mirror is enabled. Treat it like a password; do not post it publicly." }
        }
    }
}
