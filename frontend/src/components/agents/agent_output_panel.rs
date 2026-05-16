use super::agent_output_line::AgentOutputLine;
use super::agent_selector::AgentSelector;
use crate::stores::agent_output::{use_agent_output_store, OutputLine as StoreLine};
use dioxus::prelude::*;

/// Convert a store OutputLine to the component-level OutputLine.
fn to_display_line(store: &StoreLine) -> super::agent_output_line::OutputLine {
    super::agent_output_line::OutputLine {
        pane_id: store.pane_id.clone(),
        line_num: store.line_num,
        text: store.text.clone(),
        timestamp: store.timestamp,
        is_stderr: false,
    }
}

#[component]
pub fn AgentOutputPanel() -> Element {
    let mut agent_output = use_agent_output_store();

    let selected_id = agent_output.read().selected_pane_id.clone();
    let auto_scroll = agent_output.read().auto_scroll;

    let store_lines: Vec<StoreLine> = selected_id
        .as_ref()
        .and_then(|id| {
            agent_output
                .read()
                .buffers
                .iter()
                .find(|(pid, _)| pid == id)
                .map(|(_, l)| l.clone())
        })
        .unwrap_or_default();
    let lines: Vec<super::agent_output_line::OutputLine> =
        store_lines.iter().map(to_display_line).collect();

    if selected_id.is_none() {
        return rsx! {
            div {
                style: "display: flex; flex-direction: column; height: 100%;",

                div {
                    style: "padding: 6px 8px; border-bottom: 1px solid var(--border);",
                    AgentSelector {
                        on_select: move |id: String| {
                            agent_output.write().select_agent(Some(id));
                        }
                    }
                }

                div {
                    style: "flex: 1; display: flex; align-items: center; justify-content: center; color: var(--textDim);",
                    span { style: "font-size: 10px;", "Select an agent to view output" }
                }
            }
        };
    }

    let pane_id_display: String = selected_id
        .as_ref()
        .map(|s| s.chars().take(16).collect())
        .unwrap_or_default();
    let line_count = lines.len();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%;",

            // Toolbar
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                div {
                    style: "flex: 1; min-width: 0;",
                    AgentSelector {
                        on_select: move |id: String| {
                            agent_output.write().select_agent(Some(id));
                        }
                    }
                }

                // Clear button
                button {
                    style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; cursor: pointer; font-size: 9px; font-weight: 600; color: var(--textDim); letter-spacing: 0.04em;",
                    title: "Clear output",
                    onclick: move |_| {
                        let pid = agent_output.read().selected_pane_id.clone();
                        if let Some(ref id) = pid {
                            agent_output.write().clear_buffer(id);
                        }
                    },
                    "DEL"
                }

                // Scroll-to-bottom button (when auto-scroll is off)
                if !auto_scroll {
                    button {
                        style: "padding: 4px 6px; border-radius: 4px; border: none; background: transparent; cursor: pointer; font-size: 9px; font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                        title: "Scroll to bottom",
                        onclick: move |_| agent_output.write().set_auto_scroll(true),
                        "BOT"
                    }
                }
            }

            // Output lines
            div {
                style: "flex: 1; overflow-y: auto; overflow-x: hidden; background: var(--bg);",

                if lines.is_empty() {
                    div {
                        style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--textDim);",
                        span { style: "font-size: 10px;", "No output captured yet" }
                    }
                } else {
                    for line in lines.iter() {
                        AgentOutputLine {
                            key: "{line.pane_id}-{line.line_num}",
                            line: line.clone(),
                            show_line_numbers: true,
                        }
                    }
                }
            }

            // Footer
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 2px 8px; border-top: 1px solid var(--border); background: var(--bgSecondary); font-size: 9px; color: var(--textDim); flex-shrink: 0;",
                span { "{line_count} lines" }
                span { "{pane_id_display}" }
            }
        }
    }
}
