use super::shortcuts_ref::ShortcutsRef;
use super::theme_picker::ThemePicker;
use crate::components::shared::modal::Modal;
use crate::stores::ui::use_ui_store;
use crate::themes::{get_theme, AVAILABLE_FONTS};
use dioxus::prelude::*;

/* =============================================================
SettingsContent – shared by modal overlay and full-page panel
============================================================= */

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
                style: "width: 160px; flex-shrink: 0; display: flex; flex-direction: column; gap: 2px; padding: 16px 8px; border-right: 1px solid var(--border); background: var(--bg);",

                div {
                    style: "font-size: 9px; font-weight: 600; letter-spacing: 0.12em; color: var(--textDim); text-transform: uppercase; padding: 0 8px 10px 8px;",
                    "Settings"
                }

                for (label, idx) in tabs {
                    {
                        let is_active = active_tab() == idx;
                        let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                        let color = if is_active { "var(--text)" } else { "var(--textDim)" };
                        let border_left = if is_active { "2px solid var(--accent)" } else { "2px solid transparent" };
                        rsx! {
                            button {
                                key: "{label}",
                                style: "padding: 5px 10px; border-radius: 0 6px 6px 0; border: none; border-left: {border_left}; background: {bg}; color: {color}; cursor: pointer; font-size: 12px; text-align: left; transition: background 0.15s, color 0.15s; width: 100%; margin-left: -1px;",
                                onclick: move |_| active_tab.set(idx),
                                "{label}"
                            }
                        }
                    }
                }

                div { style: "flex: 1;" }
            }

            /* ── Right content area ───────────────────────── */
            div {
                style: "flex: 1; overflow-y: auto; padding: 24px 32px; min-width: 0;",

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
            width: 800,
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
            style: "display: flex; flex-direction: column; gap: 28px; max-width: 560px;",

            SectionHeader { title: "General", desc: "Configure your Athena environment" }

            div {
                style: "display: flex; flex-direction: column; gap: 14px;",

                /* Font Family */
                div {
                    div {
                        style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 8px;",
                        "Font Family"
                    }
                    div {
                        style: "display: flex; flex-wrap: wrap; gap: 6px;",
                        for font in AVAILABLE_FONTS {
                            {
                                let is_selected = *font == ui_state.read().font_family;
                                let current_theme = get_theme(ui_state.read().theme.name());
                                let bg = if is_selected { current_theme.accent } else { current_theme.bg_tertiary };
                                let fg = if is_selected { "#ffffff" } else { "var(--textMuted)" };
                                let font_str = font.to_string();
                                rsx! {
                                    button {
                                        key: "{font}",
                                        style: "padding: 4px 10px; border-radius: 4px; border: 1px solid var(--border); background: {bg}; color: {fg}; cursor: pointer; font-size: 11px; font-family: '{font}', monospace; transition: all 0.15s;",
                                        onclick: move |_| {
                                            let font_clone = font_str.clone();
                                            ui_state.write().font_family = font_clone;
                                            let size = ui_state.read().font_size;
                                            crate::themes::apply_font_to_dom(&font_str, size);
                                            let f = font_str.clone();
                                            spawn(async move {
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

                /* Font Size */
                div {
                    div {
                        style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 8px;",
                        "Font Size"
                    }
                    div {
                        style: "display: flex; align-items: center; gap: 12px;",
                        input {
                            r#type: "range",
                            min: "10",
                            max: "24",
                            value: "{ui_state.read().font_size}",
                            style: "flex: 1; accent-color: var(--accent);",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let fam = ui_state.read().font_family.clone();
                                    ui_state.write().font_size = val;
                                    crate::themes::apply_font_to_dom(&fam, val);
                                    spawn(async move {
                                        let _ = crate::tauri_bridge::store_set("font_size", &val.to_string()).await;
                                    });
                                }
                            },
                        }
                        span {
                            style: "font-size: 11px; color: var(--textMuted); min-width: 32px; text-align: center;",
                            "{ui_state.read().font_size}px"
                        }
                    }
                }

                /* Preview */
                div {
                    style: "padding: 12px; border: 1px solid var(--border); border-radius: 6px; background: var(--bgSecondary); margin-top: 8px;",
                    div {
                        style: "font-size: 10px; color: var(--textDim); margin-bottom: 6px; text-transform: uppercase; letter-spacing: 0.05em;",
                        "Preview"
                    }
                    div {
                        style: "font-family: '{ui_state.read().font_family}', monospace; font-size: {ui_state.read().font_size}px; color: var(--text); line-height: 1.6;",
                        "fn main() {{"
                        br {}
                        "    println!(\"Hello, world!\");"
                        br {}
                        "}}"
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
    let mut is_saved = use_signal(|| false);

    // Load saved values from store on mount
    use_effect(move || {
        spawn(async move {
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

        // Clear the input field immediately so the raw key never lingers in the signal
        api_key_input.set(String::new());

        spawn(async move {
            if !new_key.is_empty() {
                let _ = crate::tauri_bridge::store_set("llm.api_key", &new_key).await;
            }
            let _ = crate::tauri_bridge::store_set("llm.base_url", &url).await;
            let _ = crate::tauri_bridge::store_set("llm.model", &m).await;
        });
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 20px; max-width: 560px;",

            SectionHeader { title: "Athena", desc: "Configure your LLM provider. Athena works with any OpenAI-compatible API or Anthropic." }

            div {
                style: "display: flex; flex-direction: column; gap: 16px;",

                // API Key
                div {
                    style: "display: flex; flex-direction: column; gap: 6px;",
                    div {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        "API Key"
                    }
                    div {
                        style: "font-size: 11px; color: var(--textMuted); font-family: monospace;",
                        if api_key_set() {
                            "•••• Set"
                        } else {
                            "Not set"
                        }
                    }
                    input {
                        value: "{api_key_input}",
                        r#type: "password",
                        style: "width: 100%; padding: 6px 10px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgSecondary); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                        placeholder: "Enter new API key…",
                        oninput: move |e| { api_key_input.set(e.value()); is_saved.set(false); },
                    }
                }

                // Base URL
                div {
                    style: "display: flex; flex-direction: column; gap: 6px;",
                    div {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        "Base URL"
                    }
                    input {
                        value: "{base_url}",
                        style: "width: 100%; padding: 6px 10px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgSecondary); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                        placeholder: "https://api.openai.com/v1",
                        oninput: move |e| { base_url.set(e.value()); is_saved.set(false); },
                    }
                    div {
                        style: "font-size: 9px; color: var(--textDim);",
                        "e.g. https://api.openai.com/v1, https://api.groq.com/openai/v1, http://localhost:1234/v1"
                    }
                }

                // Model
                div {
                    style: "display: flex; flex-direction: column; gap: 6px;",
                    div {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        "Model"
                    }
                    input {
                        value: "{model}",
                        style: "width: 100%; padding: 6px 10px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgSecondary); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                        placeholder: "gpt-4o, gpt-4, llama3.1, ...",
                        oninput: move |e| { model.set(e.value()); is_saved.set(false); },
                    }
                }

                // Save button
                div {
                    style: "display: flex; align-items: center; gap: 12px; margin-top: 4px;",
                    button {
                        style: "padding: 6px 16px; border-radius: 6px; border: none; background: var(--accent); color: var(--text); cursor: pointer; font-size: 11px; font-weight: 500;",
                        onclick: move |_| {
                            do_save();
                            is_saved.set(true);
                        },
                        "Save"
                    }
                    if is_saved() {
                        span {
                            style: "font-size: 11px; color: var(--success);",
                            "✓ Saved"
                        }
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
    let mut show_form = use_signal(|| false);

    // Read agents into local variable for the render
    let agents_snapshot: Vec<crate::types::workspace::CustomAgent> =
        ui_state.read().custom_agents.clone();

    // Build the list as a Vec<Element> outside of rsx! macro
    let persist = |agents: &[_]| {
        let a = agents.to_owned();
        spawn(async move {
            if let Ok(json) = serde_json::to_string(&a) {
                let _ = crate::tauri_bridge::store_set("custom_agents", &json).await;
            }
        });
    };

    let _has_agents = !agents_snapshot.is_empty();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 20px; max-width: 560px;",

            SectionHeader { title: "Agents", desc: "Manage your agent configurations. Create custom agents with aliases and commands that launch them." }

            // List of custom agents
            div {
                style: "display: flex; flex-direction: column; gap: 8px;",

                div {
                    style: "display: flex; align-items: center; justify-content: space-between;",
                    div {
                        style: "font-size: 11px; font-weight: 600; color: var(--text);",
                        "Custom Agents"
                    }
                    button {
                        style: "padding: 4px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 11px; transition: background 0.15s;",
                        onclick: move |_| { show_form.set(true); new_alias.set(String::new()); new_command.set(String::new()); },
                        "+ Add Agent"
                    }
                }

                if show_form() {
                    div {
                        style: "padding: 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bgSecondary); display: flex; flex-direction: column; gap: 10px;",
                        div {
                            style: "font-size: 10px; color: var(--textDim); margin-bottom: 2px; text-transform: uppercase; letter-spacing: 0.05em;",
                            "New Agent"
                        }
                        input {
                            style: "width: 100%; padding: 6px 10px; border-radius: 4px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                            value: "{new_alias}",
                            placeholder: "Alias (e.g., my-claude)",
                            oninput: move |e| new_alias.set(e.value()),
                        }
                        input {
                            style: "width: 100%; padding: 6px 10px; border-radius: 4px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                            value: "{new_command}",
                            placeholder: "Command (e.g., claude --project foo)",
                            oninput: move |e| new_command.set(e.value()),
                        }
                        div {
                            style: "display: flex; gap: 8px; justify-content: flex-end; margin-top: 4px;",
                            button {
                                style: "padding: 4px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--textMuted); cursor: pointer; font-size: 11px; transition: background 0.15s;",
                                onclick: move |_| show_form.set(false),
                                "Cancel"
                            }
                            button {
                                style: if new_alias.read().trim().is_empty() || new_command.read().trim().is_empty() {
                                    "padding: 4px 10px; border-radius: 6px; border: none; background: var(--bgTertiary); color: var(--textMuted); cursor: not-allowed; font-size: 11px; opacity: 0.65;"
                                } else {
                                    "padding: 4px 10px; border-radius: 6px; border: none; background: var(--accent); color: var(--text); cursor: pointer; font-size: 11px; font-weight: 500;"
                                },
                                onclick: move |_| {
                                    let alias = new_alias.read().trim().to_string();
                                    let cmd = new_command.read().trim().to_string();
                                    if alias.is_empty() || cmd.is_empty() { return; }
                                    let new_agent = crate::types::workspace::CustomAgent {
                                        id: format!("custom-{}", js_sys::Date::now() as u64),
                                        alias: alias,
                                        command: cmd,
                                    };
                                    let mut ag = ui_state.read().custom_agents.clone();
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
                style: "display: flex; flex-direction: column; gap: 8px; margin-top: 8px;",
                CustomAgentList {}
            }

            // Predefined agents (read-only view)
            div {
                style: "margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border);",
                div {
                    style: "font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 8px;",
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
                            style: "display: flex; align-items: center; justify-content: space-between; padding: 6px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgSecondary);",
                            span {
                                style: "font-size: 11px; font-weight: 500; color: var(--text);",
                                "{name}"
                            }
                            span {
                                style: "font-size: 10px; color: var(--textDim); font-family: monospace;",
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
    let agents = ui_state.read().custom_agents.clone();

    if agents.is_empty() {
        return rsx! {
            div {
                style: "padding: 24px; text-align: center; color: var(--textDim); font-size: 11px; border: 1px dashed var(--border); border-radius: 8px;",
                "No custom agents yet. Click + Add Agent to create one."
            }
        };
    }

    rsx! {
        for agent in agents {
            CustomAgentRow { agent: agent.clone() }
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
            style: "display: flex; flex-direction: column; gap: 4px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bgSecondary); transition: border-color 0.15s;",
            div {
                style: "display: flex; align-items: center; justify-content: space-between; gap: 8px;",
                div {
                    style: "display: flex; align-items: center; gap: 8px;",
                    span {
                        style: "font-size: 12px; font-weight: 600; color: var(--text); background: var(--bgTertiary); padding: 2px 8px; border-radius: 4px; border: 1px solid var(--border);",
                        "{alias}"
                    }
                }
                button {
                    style: "padding: 2px 8px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); font-size: 11px; cursor: pointer; transition: color 0.15s;",
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
            div {
                style: "font-size: 10px; color: var(--textDim); font-family: monospace; background: var(--bg); padding: 4px 8px; border-radius: 4px; border: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
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
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 40px 20px; color: var(--textDim); max-width: 560px;",

            div {
                style: "font-size: 24px; font-weight: 700; color: var(--text); letter-spacing: -0.02em;",
                "Athena"
            }
            div {
                style: "font-size: 11px; color: var(--textMuted); margin-top: 2px;",
                "v0.1.0"
            }
            div {
                style: "font-size: 10px; color: var(--textDim); margin-top: 6px;",
                "AI-powered software orchestration and development environment"
            }
        }
    }
}

/* =============================================================
Shared primitives
============================================================= */

#[derive(Props, Clone, PartialEq)]
struct SectionHeaderProps {
    title: String,
    desc: String,
}

#[component]
fn SectionHeader(props: SectionHeaderProps) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 8px;",
            div {
                style: "font-size: 18px; font-weight: 700; color: var(--text); letter-spacing: -0.01em;",
                "{props.title}"
            }
            div {
                style: "font-size: 11px; color: var(--textDim); margin-top: 2px;",
                "{props.desc}"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SettingRowProps {
    label: String,
    value: String,
}

#[component]
fn SettingRow(props: SettingRowProps) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid var(--border);",
            span {
                style: "font-size: 11px; color: var(--text);",
                "{props.label}"
            }
            span {
                style: "font-size: 11px; color: var(--textDim);",
                "{props.value}"
            }
        }
    }
}
