//! Attention inbox: panes whose detected agent is waiting at an OSC 6337
//! "awaiting input" prompt, plus a one-click focus jump (Cmd+Shift+U also
//! routes here via the command palette). Dismissed rows stay dismissed until
//! the pane's status changes again.

use dioxus::prelude::*;

use crate::stores::agent_status::use_agent_status_store;
use crate::stores::workspace::use_workspace_store;

/// Badge count shown on the Agents tab of the sidebar.
pub fn inbox_badge_count() -> usize {
    let store = use_agent_status_store();
    let mut out = 0usize;
    for (_, s) in store.read().statuses.iter() {
        if matches!(s.status, crate::stores::agent_status::AgentRunStatus::WaitingForInput)
            && !s.dismissed
        {
            out += 1;
        }
    }
    out
}

#[component]
pub fn InboxPanel() -> Element {
    let mut agent_status = use_agent_status_store();
    let workspace = use_workspace_store();

    // Waiting panes, deduped by current workspace when possible.
    let waiting: Vec<(String, Option<String>, Option<String>)> = {
        let ws = workspace.read();
        let active_id = ws.active_space_id.clone();
        let active_dir = active_id.as_ref().and_then(|id| {
            ws.spaces.iter().find(|s| &s.id == id).map(|s| s.dir.clone())
        });
        agent_status
            .read()
            .statuses
            .iter()
            .filter(|(_, s)| matches!(s.status, crate::stores::agent_status::AgentRunStatus::WaitingForInput) && !s.dismissed)
            .map(|(pane, s)| {
                let agent_type_relevant_space = active_dir.clone();
                (pane.clone(), agent_type_relevant_space, s.message.clone())
            })
            .collect()
    };

    let mut dismiss_one = move |pane_id: String| {
        agent_status.write().dismiss_waiting(&pane_id);
    };

    let mut dismiss_all = move |_| {
        agent_status.write().dismiss_all_waiting();
    };

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow-y: auto; padding: 10px; gap: 6px;",

            div { style: "display: flex; align-items: center; justify-content: space-between; padding: 0 4px 8px 4px; border-bottom: 1px solid var(--border);",
                span { style: "font-size: var(--text-2xs); color: var(--accent); text-transform: uppercase; letter-spacing: 0.06em;", "Needs input" }
                if !waiting.is_empty() {
                    button {
                        class: "btn-ghost",
                        title: "Mark all as seen",
                        style: "font-size: 10px; padding: 2px 8px;",
                        onclick: dismiss_all,
                        "Clear all"
                    }
                }
            }

            if waiting.is_empty() {
                div { style: "padding: 20px 8px; text-align: center; font-size: 11px; color: var(--textMuted);", "No agent is waiting on input right now." }
            } else {
                for (pane_id, _space, message) in waiting.iter() {
                    {
                        let id = pane_id.clone();
                        rsx! {
                            div { key: "{id}", style: "display: flex; align-items: center; gap: 8px; padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgSecondary);",
                                span { style: "width: 6px; height: 6px; border-radius: 3px; background: var(--warning); flex-shrink: 0;" }
                                div { style: "flex: 1; min-width: 0;",
                                    span { style: "display: block; font-size: 11px; color: var(--text); font-family: var(--font-mono);", "{pane_id}" }
                                    if let Some(msg) = message.clone() {
                                        span { style: "font-size: 10px; color: var(--textDim);", "{msg}" }
                                    }
                                }
                                button {
                                    class: "btn-ghost",
                                    style: "font-size: 9px; padding: 2px 6px; color: var(--error);",
                                    onclick: { let id2 = id.clone(); move |_| dismiss_one(id2.clone()) },
                                    "Dismiss"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
