use crate::components::shared::icon::IconChevronDown;
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
                style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; font-size: var(--text-xs); color: var(--textDim);",
                span { style: "width: 6px; height: 6px; border-radius: var(--radius-pill); opacity: 0.4; background: var(--textDim);" }
                span {
                    style: "font-family: var(--font-display); letter-spacing: 0.04em;",
                    "No agents with output"
                }
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
        .unwrap_or_else(|| "Select agent…".to_string());
    let selected_color: String = selected
        .as_ref()
        .map(|a| get_agent_color(&a.agent_type).to_string())
        .unwrap_or_else(|| "var(--textDim)".to_string());

    let _chevron_rotation: i32 = if open() { 180 } else { 0 };

    rsx! {
        div { style: "position: relative;",

            button {
                class: "lit-sweep",
                style: "display: flex; align-items: center; gap: 7px; padding: 4px 8px; border-radius: var(--radius-sm); border: 1px solid var(--border); cursor: pointer; width: 100%;",
                onclick: move |_| open.set(!open()),

                span {
                    style: "width: 7px; height: 7px; border-radius: var(--radius-pill); background: {selected_color}; flex-shrink: 0;",
                }

                span {
                    style: "font-size: var(--text-sm); font-weight: 500; font-family: var(--font-display); letter-spacing: 0.04em; flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text);",
                    "{selected_display}"
                }

                IconChevronDown { size: Some(13), color: Some("var(--textDim)".to_string()) }
            }

            if open() {
                div {
                    style: "position: absolute; top: 100%; left: 0; right: 0; z-index: 50; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); box-shadow: var(--shadow-md); max-height: 240px; overflow-y: auto; margin-top: 4px;",

                    for agent in agents.iter() {
                        {
                            let pane_id_for_event = agent.pane_id.clone();
                            let is_selected = Some(agent.pane_id.as_str()) == selected_id.as_deref();
                            let color = get_agent_color(&agent.agent_type).to_string();
                            let label = get_agent_label(&agent.agent_type).to_string();
                            let display_id: String = agent.pane_id.chars().take(12).collect();
                            let lc = agent.line_count;
                            let item_bg = "transparent";
                            let item_text_color = if is_selected { "var(--accent)" } else { "var(--textDim)" };
                            let id_text_color = if is_selected { "var(--accent)" } else { "var(--textDim)" };
                            let color_bg = format!("{}22", color);
                            let pane_id_for_select = agent.pane_id.clone();
                            rsx! {
                                button {
                                    key: "{agent.pane_id}",
                                    class: "agent-selector-row lit-sweep",
                                    style: "display: flex; align-items: center; gap: 8px; padding: 7px 12px; width: 100%; text-align: left; border: none; border-bottom: 1px solid var(--border); background: {item_bg}; cursor: pointer;",
                                    onclick: move |_| {
                                        props.on_select.call(pane_id_for_event.clone());
                                        agent_output.write().select_agent(Some(pane_id_for_select.clone()));
                                        open.set(false);
                                    },

                                    span {
                                        style: "width: 7px; height: 7px; border-radius: var(--radius-pill); background: {color}; flex-shrink: 0;",
                                    }

                                    span {
                                        style: "font-size: var(--text-2xs); font-family: var(--fontFamily); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: {id_text_color};",
                                        "{display_id}"
                                    }

                                    span {
                                        class: "badge",
                                        style: "background: {color_bg}; color: {color};",
                                        "{label}"
                                    }

                                    span {
                                        style: "font-size: var(--text-2xs); color: {item_text_color};",
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
