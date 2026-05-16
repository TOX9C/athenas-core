use crate::stores::agent_output::use_agent_output_store;
use dioxus::prelude::*;

/// Get a color for an agent type.
fn get_agent_color(agent_type: &str) -> &'static str {
    match agent_type {
        "claude" => "#f97316",
        "codex" => "#10b981",
        "opencode" => "#8b5cf6",
        "gemini" => "#3b82f6",
        "shell" => "#6b7280",
        _ => "var(--accent)",
    }
}

/// Get a label for an agent type.
fn get_agent_label(agent_type: &str) -> &'static str {
    match agent_type {
        "claude" => "Claude",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "gemini" => "Gemini",
        "shell" => "Shell",
        _ => "Agent",
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct AgentSelectorProps {
    pub on_select: EventHandler<String>,
}

#[component]
pub fn AgentSelector(props: AgentSelectorProps) -> Element {
    let mut agent_output = use_agent_output_store();
    let mut open = use_signal(|| false);

    let agents = agent_output.read().agents.clone();
    let selected_id = agent_output.read().selected_pane_id.clone();

    if agents.is_empty() {
        return rsx! {
            div {
                style: "display: flex; align-items: center; gap: 6px; padding: 2px 8px; font-size: 10px; color: var(--textDim);",
                span { style: "width: 5px; height: 5px; border-radius: 50%; opacity: 0.3; background: var(--textDim);" }
                "No agents with output"
            }
        };
    }

    let selected = agents
        .iter()
        .find(|a| Some(a.pane_id.as_str()) == selected_id.as_deref())
        .cloned();

    let selected_display: String = selected
        .as_ref()
        .map(|a| a.pane_id.chars().take(12).collect())
        .unwrap_or_else(|| "Select agent...".to_string());
    let selected_color: String = selected
        .as_ref()
        .map(|a| get_agent_color(&a.agent_type).to_string())
        .unwrap_or_else(|| "var(--textDim)".to_string());

    let chevron_rotation: i32 = if open() { 180 } else { 0 };

    rsx! {
        div { style: "position: relative;",

            button {
                style: "display: flex; align-items: center; gap: 6px; padding: 2px 8px; border-radius: 4px; border: none; background: transparent; cursor: pointer; width: 100%;",
                onclick: move |_| open.set(!open()),

                span {
                    style: "width: 6px; height: 6px; border-radius: 50%; background: {selected_color};",
                }

                span {
                    style: "font-size: 11px; font-weight: 500; flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text);",
                    "{selected_display}"
                }

                span {
                    style: "font-size: 9px; color: var(--textDim); transition: transform 0.15s; transform: rotate({chevron_rotation}deg);",
                    "\u{25be}"
                }
            }

            if open() {
                div {
                    style: "position: absolute; top: 100%; left: 0; right: 0; z-index: 50; border: 1px solid var(--border); border-radius: 6px; background: var(--bgSecondary); max-height: 240px; overflow-y: auto; box-shadow: 0 8px 24px rgba(0,0,0,0.3);",

                    for agent in agents.iter() {
                        {
                            let pane_id_for_event = agent.pane_id.clone();
                            let is_selected = Some(agent.pane_id.as_str()) == selected_id.as_deref();
                            let color = get_agent_color(&agent.agent_type).to_string();
                            let label = get_agent_label(&agent.agent_type).to_string();
                            let display_id: String = agent.pane_id.chars().take(12).collect();
                            let lc = agent.line_count;
                            let item_bg = if is_selected { "var(--bgTertiary)" } else { "transparent" };
                            let color_bg = format!("{}22", color);
                            let pane_id_for_select = agent.pane_id.clone();
                            rsx! {
                                button {
                                    key: "{agent.pane_id}",
                                    style: "display: flex; align-items: center; gap: 8px; padding: 6px 12px; width: 100%; text-align: left; border: none; background: {item_bg}; cursor: pointer;",
                                    onclick: move |_| {
                                        props.on_select.call(pane_id_for_event.clone());
                                        agent_output.write().select_agent(Some(pane_id_for_select.clone()));
                                        open.set(false);
                                    },

                                    span {
                                        style: "width: 6px; height: 6px; border-radius: 50%; background: {color};",
                                    }

                                    span {
                                        style: "font-size: 11px; font-weight: 500; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text);",
                                        "{display_id}"
                                    }

                                    span {
                                        style: "font-size: 9px; padding: 1px 4px; border-radius: 3px; background: {color_bg}; color: {color};",
                                        "{label}"
                                    }

                                    span {
                                        style: "font-size: 8px; color: var(--textDim);",
                                        "{lc} lines"
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
