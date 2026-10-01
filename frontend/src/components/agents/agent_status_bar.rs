use crate::components::shared::icon::IconPulse;
use crate::stores::agent_queue::use_agent_queue_store;
use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::use_agent_status_store;
use crate::utils::agent_display::{get_agent_color_str, get_agent_display_name};
use dioxus::prelude::*;

#[path = "agent_status_bar_model.rs"]
mod agent_status_bar_model;

use agent_status_bar_model::{status_label, time_ago, to_pane_status};
pub use agent_status_bar_model::{AgentPaneStatus, ProgressInfo};

#[derive(Props, Clone, PartialEq)]
pub struct AgentStatusBarProps {
    pub pane_id: String,
}

#[component]
pub fn AgentStatusBar(props: AgentStatusBarProps) -> Element {
    let agent_status = use_agent_status_store();
    let agent_output = use_agent_output_store();

    let current_status: AgentPaneStatus = agent_status
        .read()
        .statuses
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| to_pane_status(s))
        .unwrap_or_default();

    let mut queue = use_agent_queue_store();
    let mut queue_input = use_signal(String::new);
    let is_agent = !current_status.agent_type.is_empty();
    let busy = matches!(current_status.status.as_str(), "thinking" | "working");
    let queued_count = queue.read().queued(&props.pane_id).len();

    // Deliver queued messages once the agent returns to a prompt: a
    // transition from busy (thinking/working) → anything else drains the
    // queue in order into the pane's PTY.
    let mut was_busy = use_signal(|| false);
    {
        let pane_id = props.pane_id.clone();
        use_effect(move || {
            let was = was_busy();
            was_busy.set(busy);
            if was && !busy && !queue.read().is_empty(&pane_id) {
                let mut queue = queue;
                let pane_id = pane_id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    while let Some(msg) = queue.write().take_next(&pane_id) {
                        let _ = crate::tauri_bridge::pty_write(
                            &pane_id,
                            &format!("{}\n", msg.text),
                        )
                        .await;
                    }
                });
            }
        });
    }

    let line_count: usize = agent_output
        .read()
        .buffers
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, lines)| lines.len())
        .unwrap_or(0);

    let (label, word, color) = status_label(&current_status.status);
    let agent_color = get_agent_color_str(&current_status.agent_type);
    let display_id: String = get_agent_display_name(&current_status.agent_type, &props.pane_id);
    let pane_id_full = props.pane_id.clone();
    let msg_preview: String = current_status.message.chars().take(40).collect();
    let ago = time_ago(current_status.last_updated_at);

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-top: 1px solid var(--border); flex-shrink: 0; overflow-x: hidden;",

            // Agent helmet glyph
            span {
                style: "display: inline-flex; align-items: center; color: {agent_color}; flex-shrink: 0;",
                IconPulse { size: Some(14), color: Some("currentColor".to_string()) }
            }

            // Status label. The word carries the state without a capsule or
            // accent marker competing with the agent identity glyph.
            span {
                class: "status-label",
                style: "color: {color}; flex-shrink: 0;",
                span {
                    style: "font-family: var(--font-display);",
                    title: "{label}",
                    "{word}"
                }
            }

            // Pane id (human label; hover reveals the raw pane id)
            span {
                style: "font-size: var(--text-2xs); font-family: var(--fontFamily); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--textMuted); flex-shrink: 0;",
                title: "{pane_id_full}",
                "{display_id}"
            }

            // Message preview
            if !current_status.message.is_empty() {
                span {
                    style: "font-size: var(--text-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; color: var(--textDim);",
                    "{msg_preview}"
                }
            }

            // Line count
            if line_count > 0 {
                span {
                    style: "font-size: var(--text-2xs); flex-shrink: 0; color: var(--textDim);",
                    "{line_count} lines"
                }
            }

            // Message queue affordance (agent panes only).
            if is_agent {
                span { style: "flex: 0 0 auto", } // spacer
                span {
                    style: "font-size: var(--text-2xs); color: var(--textDim);",
                    title: "Queued messages deliver when the agent returns to the prompt",
                    "⧗{queued_count}"
                }
            }

            // Time ago
            span {
                style: "font-size: var(--text-2xs); flex-shrink: 0; color: var(--textDim);",
                "{ago}"
            }
        }

        // Queue composer rows (visible only for agent panes).
        if is_agent {
            div { style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-top: 1px solid var(--border); background: var(--bgSecondary);",
                input {
                    style: "flex: 1; min-width: 0; font-size: 11px; padding: 4px 6px; background: var(--bgTertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text);",
                    placeholder: if busy { "Message queues until the prompt returns…" } else { "Message the agent…" },
                    value: "{queue_input}",
                    oninput: move |e| queue_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) {
                            e.prevent_default();
                            let text = queue_input.read().clone();
                            queue_input.set(String::new());
                            crate::components::agents::agent_status_bar_helpers::queue_transfer(
                                queue,
                                &props.pane_id,
                                &text,
                                busy,
                            );
                        }
                    },
                }
            }
            {
                let queued = queue.read().queued(&props.pane_id);
                if queued.is_empty() {
                    rsx! {}
                } else {
                    rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 2px; padding: 4px 8px; border-top: 1px dashed var(--border); background: var(--bgSecondary);",
                            {
                                let pane_id_for_list = props.pane_id.clone();
                                rsx! {
                                    for msg in queued.iter() {
                                        {
                                            let id = msg.id.clone();
                                            let text = msg.text.clone();
                                            let pane_id_for_row = pane_id_for_list.clone();
                                            rsx! {
                                                div { key: "{id}", style: "display: flex; align-items: center; gap: 4px; font-size: 10px; color: var(--textMuted);",
                                                    span { style: "flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{text}" }
                                                    button { class: "btn-ghost", style: "font-size: 9px; padding: 0 4px;", onclick: { let id = id.clone(); let pane_id = pane_id_for_row.clone(); move |_| queue.write().move_up(&pane_id, &id) }, "↑" }
                                                    button { class: "btn-ghost", style: "font-size: 9px; padding: 0 4px;", onclick: { let id = id.clone(); let pane_id = pane_id_for_row.clone(); move |_| queue.write().move_down(&pane_id, &id) }, "↓" }
                                                    button { class: "btn-ghost", style: "font-size: 9px; padding: 0 4px; color: var(--error);", onclick: { let id = id.clone(); let pane_id = pane_id_for_row.clone(); move |_| queue.write().remove(&pane_id, &id) }, "×" }
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
        }
    }
}
