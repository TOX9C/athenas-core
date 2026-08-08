//! Settings section components for General, Athena, and About.

use super::{FontDropdown, GroupLabel, LabeledField, SizeStepper, Toggle};
use crate::stores::athena::use_athena_store;
use crate::stores::ui::use_ui_store;
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
pub(super) fn AthenaSettings() -> Element {
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
        /* GroupLabel "Provider" sits first to anchor the first labeled field. */
        GroupLabel { label: "Provider", first: true }

        LabeledField {
            label: "API Key",
            description: Some("Stored in the OS keychain. Paste to replace."),
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
                oninput: move |e| { base_url.set(e.value()); save_status.set(None); },
            }
        }

        LabeledField {
            label: "Model",
            description: Some("gpt-4o, claude-sonnet-4-6, llama3.1, …"),
            input {
                value: "{model}",
                class: "field",
                style: "width: 100%; box-sizing: border-box;",
                placeholder: "gpt-4o, gpt-4, llama3.1, …",
                oninput: move |e| { model.set(e.value()); save_status.set(None); },
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
                "Save ↵"
            }
            match save_status() {
                Some(true) => rsx! {
                    span {
                        style: "display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--success); font-weight: 500;",
                        "✓ Saved"
                    }
                },
                Some(false) => rsx! {
                    span {
                        style: "display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--error); font-weight: 500;",
                        "✕ Save failed — see notification"
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

#[component]
pub(super) fn AboutSettings() -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; padding: 36px 20px; color: var(--textDim); max-width: 620px; margin: 0 auto;",
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
    let navigator = js_sys::Reflect::get(&window, &JsValue::from_str("navigator")).map_err(|_| ())?;
    let clipboard = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")).map_err(|_| ())?;
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
                    span {
                        style: "font-size: 11px; color: var(--error); margin-top: 2px; word-break: break-word;",
                        "{last_error.read()}"
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
