use super::agent_output_line::AgentOutputLine;
use super::agent_selector::AgentSelector;
use crate::components::shared::icon::{IconChevronDown, IconTrash};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_output::{use_agent_output_store, OutputLine};
use crate::utils::agent_display::get_agent_display_name;
use dioxus::prelude::*;

/// Keep the visible window bounded — the buffer can grow to thousands of
/// lines.
const MAX_VISIBLE_LINES: usize = 500;

#[component]
pub fn AgentOutputPanel() -> Element {
    let agent_output = use_agent_output_store();

    // `AgentOutputStore` is `Clone` (cheap: one `Rc` map + `Copy` signals).
    // Clone one handle per capturing closure below — the store is not `Copy`,
    // and each `move` closure would otherwise fight over the same value.
    let for_lines = agent_output.clone();
    let for_selected = agent_output.clone();
    let for_select_empty = agent_output.clone();
    let for_select_toolbar = agent_output.clone();
    let for_clear = agent_output.clone();
    let for_scroll = agent_output.clone();

    // Subscribe to the *selected pane's own* buffer signal (not the whole
    // store): a batch for pane B never re-evaluates this memo while pane A
    // is selected. The memo returns the visible tail window; `OutputLine`
    // clones are refcount bumps (`Rc<str>` text).
    //
    // NOTE: the memo must be registered unconditionally (before the early
    // return below) — governed-hook-order, same reason as AthenaPanel.
    let lines = use_memo(move || {
        let pid = for_lines.selected_pane_id_signal().read().clone()?;
        let buffer = for_lines.buffer_signal(&pid)?;
        let guard = buffer.read();
        let start = guard.len().saturating_sub(MAX_VISIBLE_LINES);
        Some(guard[start..].to_vec())
    });

    let selected_id = use_memo(move || for_selected.selected_pane_id_signal().read().clone());
    let auto_scroll_signal = agent_output.auto_scroll_signal();
    let auto_scroll = auto_scroll_signal.read();

    if selected_id().is_none() {
        return rsx! {
            div {
                class: "pane-astrolabe-mark",
                style: "display: flex; flex-direction: column; height: 100%; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md);",

                div {
                    style: "padding: 8px 10px; border-bottom: 1px solid var(--border);",
                    AgentSelector {
                        on_select: move |id: String| {
                            for_select_empty.select_agent(Some(id));
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

    let pane_id_display: String = {
        let pane_id = agent_output.selected_pane_id_signal().peek().clone();
        pane_id
            .as_deref()
            .and_then(|pid| {
                agent_output
                    .agents_signal()
                    .read()
                    .iter()
                    .find(|a| a.pane_id == pid)
                    .map(|a| get_agent_display_name(&a.agent_type, pid))
            })
            .unwrap_or_else(|| {
                pane_id
                    .as_deref()
                    .map(|pid| pid.chars().take(16).collect())
                    .unwrap_or_default()
            })
    };
    let pane_id_full = selected_id().unwrap_or_default();
    // Snapshot the memoized window once — below we touch the list three
    // times (count, empty-check, loop). Clones are `Rc` refcount bumps.
    let lines_win: Vec<OutputLine> = lines().unwrap_or_default();
    let line_count = lines_win.len();

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
                            for_select_toolbar.select_agent(Some(id));
                        }
                    }
                }

                // Clear button
                button {
                    class: "icon-btn",
                    title: "Clear output",
                    onclick: move |_| {
                        let pid = for_clear.selected_pane_id_signal().peek().clone();
                        if let Some(id) = &pid {
                            for_clear.clear_buffer(id);
                        }
                    },
                    IconTrash { size: Some(15), color: Some("var(--error)".to_string()) }
                }

                // Scroll-to-bottom button (when auto-scroll is off)
                if !*auto_scroll {
                    button {
                        class: "icon-btn is-active",
                        title: "Scroll to bottom",
                        onclick: move |_| for_scroll.set_auto_scroll(true),
                        IconChevronDown { size: Some(15), color: Some("currentColor".to_string()) }
                    }
                }
            }

            // Output lines
            div {
                style: "flex: 1; overflow-y: auto; overflow-x: hidden; background: var(--bg);",

                if lines_win.is_empty() {
                    EmptyState {
                        kind: EmptyArt::Generic,
                        title: "No output".to_string(),
                        hint: Some("Agent output will stream here.".to_string()),
                    }
                } else {
                    for line in lines_win.iter() {
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
                style: "display: flex; align-items: center; gap: 4px; justify-content: space-between; padding: 4px 10px; border-top: 1px solid var(--border); font-size: var(--text-2xs); color: var(--textDim); font-family: var(--fontFamily); flex-shrink: 0;",
                span { "{line_count} lines (latest {MAX_VISIBLE_LINES} shown)" }
                span { title: "{pane_id_full}", "{pane_id_display}" }
            }
        }
    }
}
