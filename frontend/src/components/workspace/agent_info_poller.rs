//! Central agent-info poller.
//!
//! A single long-lived component (mounted once at the app root, next to
//! `OutputEventBus`) that periodically calls `pty_agent_info` for every pane
//! in the active space and writes the result into the terminal store.
//!
//! This is what makes the "is Claude/Codex running?" detection and the
//! scraped task titles actually show up in pane pills — the store fields
//! (`session.foreground_process`, `session.task_title`) are read by `PaneItem`
//! but were never written before this component existed.
//!
//! It intentionally runs on a slower cadence (1500ms) than the per-pane
//! 750ms loop in `PaneItem`, which is purpose-built for resume-banner reveal
//! gating and stays untouched. This loop's only job is store writes; the
//! change-detection guard in `TerminalStore::update_agent_info` prevents
//! spurious re-renders when nothing changed.

use dioxus::prelude::*;

use crate::stores::terminal::use_terminal_store;
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge::{pty_agent_info, summarize_agent_title};

/// Poll interval. `pty_agent_info` shells out to `ps` and (for Claude/Codex
/// panes) re-reads the agent's history file, so a slower cadence keeps the
/// process-table churn and file reads reasonable across many panes.
const POLL_INTERVAL_MS: u64 = 1500;

/// Mount-once component. Renders nothing — exists only to own the `use_future`.
#[allow(non_snake_case)]
pub fn AgentInfoPoller() -> Element {
    let terminal_store = use_terminal_store();
    let workspace = use_workspace_store();
    let ui_state = use_ui_store();

    // Track which session IDs we have already summarized (per-pane).
    let summarized_sessions: Signal<std::collections::HashSet<String>> =
        use_signal(std::collections::HashSet::new);

    use_future(move || {
        let mut terminal_store = terminal_store.clone();
        let workspace = workspace.clone();
        let mut summarized_sessions = summarized_sessions.clone();
        async move {
            loop {
                // Snapshot the pane ids of every space (not just the active
                // one) so a background space keeps its detection fresh too.
                let pane_ids: Vec<String> = {
                    let ws = workspace.read();
                    ws.spaces
                        .iter()
                        .flat_map(|s| s.panes.iter().map(|p| p.id.clone()))
                        .collect()
                };

                for pane_id in &pane_ids {
                    match pty_agent_info(pane_id).await {
                        Ok(info) => {
                            let fg = if info.foreground_process.is_empty()
                                || info.foreground_process == "shell"
                            {
                                None
                            } else {
                                Some(info.foreground_process.clone())
                            };

                            // Trigger LLM summarization for a new session if:
                            // the feature is enabled AND we haven't seen this session yet.
                            let sid = info.session_id.as_deref().unwrap_or_default();
                            let feature_enabled = ui_state.read().smart_pane_titles;
                            if feature_enabled
                                && !sid.is_empty()
                                && !summarized_sessions.read().contains(sid)
                            {
                                summarized_sessions.write().insert(sid.to_string());
                                if let Some(raw_prompt) = info.raw_prompt.as_ref() {
                                    let raw_prompt = raw_prompt.clone();
                                    let mut store = terminal_store.clone();
                                    let pane = pane_id.clone();
                                    // Fire-and-forget; latency does not matter.
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match summarize_agent_title(&raw_prompt).await {
                                            Ok(summary) => {
                                                let cleaned = summary.trim().to_string();
                                                web_sys::console::log_1(
                                                    &format!(
                                                        "[AgentInfoPoller] summary for pane={}: {}",
                                                        pane, cleaned
                                                    )
                                                    .into(),
                                                );
                                                // Write summarized title into the session.
                                                let mut store_guard = store.write();
                                                if let Some(session) =
                                                    store_guard.sessions.get_mut(&pane)
                                                {
                                                    session.summarized_title = Some(cleaned);
                                                    session.generation =
                                                        session.generation.wrapping_add(1);
                                                }
                                            }
                                            Err(e) => {
                                                web_sys::console::warn_1(
                                                    &format!(
                                                        "[AgentInfoPoller] summarize error: {:?}",
                                                        e
                                                    )
                                                    .into(),
                                                );
                                            }
                                        }
                                    });
                                }
                            }

                            terminal_store.write().update_agent_info(
                                pane_id,
                                fg,
                                info.task_title.clone(),
                                info.session_id.clone(),
                                info.raw_prompt.clone(),
                            );
                        }
                        Err(_) => {}
                    }
                }

                gloo::timers::future::TimeoutFuture::new(POLL_INTERVAL_MS as u32).await;
            }
        }
    });

    rsx! {}
}
