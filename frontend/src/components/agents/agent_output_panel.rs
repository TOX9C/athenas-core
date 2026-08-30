use super::agent_output_line::AgentOutputLine;
use super::agent_selector::AgentSelector;
use crate::components::shared::icon::{IconChevronDown, IconTrash};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_output::{use_agent_output_store, OutputLine as StoreLine};
use crate::utils::agent_display::get_agent_display_name;
use dioxus::prelude::*;

/// Convert a store OutputLine to the component-level OutputLine.
fn to_display_line(store: &StoreLine) -> super::agent_output_line::OutputLine {
    super::agent_output_line::OutputLine {
        pane_id: store.pane_id.clone(),
        line_num: store.line_num,
        text: store.text.clone(),
        timestamp: store.timestamp,
        is_stderr: store.is_stderr,
    }
}

#[component]
pub fn AgentOutputPanel() -> Element {
    let mut agent_output = use_agent_output_store();

    let selected_id = agent_output.read().selected_pane_id.clone();
    let auto_scroll = agent_output.read().auto_scroll;

    if selected_id.is_none() {
        return rsx! {
            div {
                class: "pane-astrolabe-mark",
                style: "display: flex; flex-direction: column; height: 100%; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md);",

                div {
                    style: "padding: 8px 10px; border-bottom: 1px solid var(--border);",
                    AgentSelector {
                        on_select: move |id: String| {
                            agent_output.write().select_agent(Some(id));
                        }
                    }
                }

                EmptyState {
                    kind: EmptyArt::Agents,
                    title: "No agent".to_string(),
                    hint: Some("Select an agent to view its output.".to_string()),
                }
            }
        };
    }

    // Convert the selected buffer's lines once per signal change. Avoids cloning
    // the entire `Vec<StoreLine>` on every render — the previous code cloned the
    // buffer Vec (deep-cloning every `text` and `pane_id` String) and then cloned
    // each string a second time during the `to_display_line` map. This memo
    // iterates the store buffer by reference, so strings are cloned exactly once
    // and the per-render allocation is skipped when nothing has changed.
    let lines = use_memo(move || {
        let store = agent_output.read();
        match store.selected_pane_id.as_deref() {
            Some(pid) => store
                .buffers
                .iter()
                .find(|(k, _)| k.as_str() == pid)
                .map(|(_, v)| v.iter().map(to_display_line).collect::<Vec<_>>())
                .unwrap_or_default(),
            None => Vec::new(),
        }
    });

    let pane_id_display: String = {
        let store = agent_output.read();
        let pane_id = store.selected_pane_id.as_deref();
        pane_id
            .and_then(|pid| {
                store
                    .agents
                    .iter()
                    .find(|a| a.pane_id == pid)
                    .map(|a| get_agent_display_name(&a.agent_type, pid))
            })
            .unwrap_or_else(|| {
                pane_id
                    .map(|pid| pid.chars().take(16).collect())
                    .unwrap_or_default()
            })
    };
    let pane_id_full = selected_id.clone().unwrap_or_default();
    let line_count = lines().len();

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md);",

            // Toolbar
            div {
                style: "display: flex; align-items: center; gap: 6px; padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

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
                    class: "icon-btn",
                    title: "Clear output",
                    onclick: move |_| {
                        let pid = agent_output.read().selected_pane_id.clone();
                        if let Some(ref id) = pid {
                            agent_output.write().clear_buffer(id);
                        }
                    },
                    IconTrash { size: Some(15), color: Some("var(--error)".to_string()) }
                }

                // Scroll-to-bottom button (when auto-scroll is off)
                if !auto_scroll {
                    button {
                        class: "icon-btn is-active",
                        title: "Scroll to bottom",
                        onclick: move |_| agent_output.write().set_auto_scroll(true),
                        IconChevronDown { size: Some(15), color: Some("currentColor".to_string()) }
                    }
                }
            }

            // Output lines
            div {
                style: "flex: 1; overflow-y: auto; overflow-x: hidden; background: var(--bg);",

                if lines().is_empty() {
                    EmptyState {
                        kind: EmptyArt::Generic,
                        title: "No output".to_string(),
                        hint: Some("Agent output will stream here.".to_string()),
                    }
                } else {
                    for line in lines().iter() {
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
                style: "display: flex; align-items: center; justify-content: space-between; padding: 4px 10px; border-top: 1px solid var(--border); font-size: var(--text-2xs); color: var(--textDim); font-family: var(--fontFamily); flex-shrink: 0;",
                span { "{line_count} lines" }
                span { title: "{pane_id_full}", "{pane_id_display}" }
            }
        }
    }
}
