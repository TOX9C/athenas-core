//! Custom agent settings and editor UI.

use dioxus::prelude::*;

use crate::components::settings::settings_modal::Toggle;
use crate::stores::ui::use_ui_store;

/// Per-type agent notification toggles, persisted under the KV key
/// `"agent_notify_config"` (the backend heartbeat applies it). Field names
/// must match `athena_core::agent_activity::AgentNotifyConfig` (snake_case).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentNotifyConfig {
    pub finished: bool,
    pub needs_attention: bool,
    pub error: bool,
}

impl Default for AgentNotifyConfig {
    fn default() -> Self {
        Self {
            finished: true,
            needs_attention: true,
            error: true,
        }
    }
}

/// Load the persisted agent-notify config (falls back to defaults).
/// Mirrors the backend default so a first-run app and a fresh KV agree.
pub(crate) async fn load_agent_notify_config() -> AgentNotifyConfig {
    match crate::tauri_bridge::store_get("agent_notify_config").await {
        Ok(json) if !json.is_empty() => serde_json::from_str(&json).unwrap_or_default(),
        _ => AgentNotifyConfig::default(),
    }
}

/// Persist the agent-notify config to the KV store (best-effort).
pub(crate) fn save_agent_notify_config(cfg: AgentNotifyConfig) {
    if let Ok(json) = serde_json::to_string(&cfg) {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = crate::tauri_bridge::store_set("agent_notify_config", &json).await;
        });
    }
}

/* =============================================================
Tab: Agents
============================================================= */

#[component]
pub(crate) fn AgentsSettings() -> Element {
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
            style: "display: flex; flex-direction: column; gap: 18px; max-width: 620px;",

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
                            Toggle {
                                active: new_is_claude(),
                                on_toggle: move |_| new_is_claude.set(!new_is_claude()),
                            }
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
                                Toggle {
                                    active: new_priority(),
                                    on_toggle: move |_| new_priority.set(!new_priority()),
                                }
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
                                        id: format!("custom-{}", crate::utils::time::now_ms()),
                                        alias,
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

            // Agent notification toggles — persisted per type; the backend
            // heartbeat applies them (finished / needs attention / error).
            AgentNotifySettings {}

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
                        ("Qwen Code", "qwen"),
                        ("Aider", "aider"),
                        ("Cursor CLI", "cursor"),
                        ("Freebuff", "freebuff"),
                        ("OMP (oh my pi)", "omp"),
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

/// Agent notification toggles (finished / needs attention / error). Loads
/// the persisted config on mount and saves on any toggle. Mirrors the
/// backend `AgentNotifyConfig` semantics: status badges always update; only
/// the *notifications* are gated per type.
#[component]
fn AgentNotifySettings() -> Element {
    let mut cfg = use_signal(AgentNotifyConfig::default);

    // Load the persisted config once on mount.
    use_effect(move || {
        spawn(async move {
            let loaded = load_agent_notify_config().await;
            cfg.set(loaded);
        });
    });

    // Each row flips one field, persists, and re-renders. The mutate helper
    // is inline per row (no shared closure) so the `Signal` stays `Copy`-
    // captured and no borrow escapes the event handler.
    let set_finished = move |_| {
        let mut next = cfg();
        next.finished = !next.finished;
        cfg.set(next);
        save_agent_notify_config(next);
    };
    let set_attention = move |_| {
        let mut next = cfg();
        next.needs_attention = !next.needs_attention;
        cfg.set(next);
        save_agent_notify_config(next);
    };
    let set_error = move |_| {
        let mut next = cfg();
        next.error = !next.error;
        cfg.set(next);
        save_agent_notify_config(next);
    };

    rsx! {
        div {
            style: "margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border);",
            div {
                style: "display: flex; align-items: center; gap: 6px; margin-bottom: 8px;",
                div {
                    style: "font-family: var(--font-display); font-size: var(--text-sm); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                    "Agent Notifications"
                }
            }
            div {
                style: "display: flex; flex-direction: column; gap: 2px;",
                NotifyRow {
                    label: "When an agent finishes work",
                    desc: "Working → finished transition (in-app + macOS)",
                    active: cfg().finished,
                    on_toggle: set_finished,
                }
                NotifyRow {
                    label: "When an agent needs attention",
                    desc: "Waiting for input / asking a question",
                    active: cfg().needs_attention,
                    on_toggle: set_attention,
                }
                NotifyRow {
                    label: "When an agent errors",
                    desc: "Error transitions while working",
                    active: cfg().error,
                    on_toggle: set_error,
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NotifyRowProps {
    label: String,
    desc: String,
    active: bool,
    on_toggle: EventHandler<bool>,
}

#[component]
fn NotifyRow(props: NotifyRowProps) -> Element {
    rsx! {
        div {
            // The nested Toggle is the semantic keyboard-accessible control;
            // keep this row as a mouse-friendly hit area rather than adding a
            // second interactive role around a button.
            style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; cursor: pointer; user-select: none; padding: 6px 0;",
            onclick: move |_| props.on_toggle.call(!props.active),
            div {
                style: "display: flex; flex-direction: column; gap: 2px; min-width: 0; padding-right: 8px;",
                span {
                    style: "font-size: var(--text-sm); font-weight: 500; color: var(--text);",
                    "{props.label}"
                }
                span {
                    style: "font-size: var(--text-xs); color: var(--textDim);",
                    "{props.desc}"
                }
            }
            Toggle {
                active: props.active,
                on_toggle: move |e: dioxus::prelude::MouseEvent| {
                    // The knob is INSIDE the clickable row — without
                    // stopping propagation the row's `onclick` would also
                    // fire, toggling twice (net no-op).
                    e.stop_propagation();
                    props.on_toggle.call(!props.active);
                },
            }
        }
    }
}
