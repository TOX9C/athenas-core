//! Central agent-info poller.
//!
//! A single long-lived component (mounted once at the app root, next to
//! `OutputEventBus`) that periodically calls `pty_agent_info` for every pane
//! in the active space and writes the result into the terminal store.
//!
//! This is what makes the "is Claude/Codex running?" detection and the
//! scraped task titles actually show up in pane pills.
//!
//! It intentionally runs on a slower cadence (1500ms) than the per-pane
//! 750ms loop in `PaneItem`, which is purpose-built for resume-banner reveal
//! gating and stays untouched. This loop's only job is store writes; the
//! change-detection guard in `TerminalStore::update_agent_info` prevents
//! spurious re-renders when nothing changed.

use dioxus::prelude::*;

use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge::{pty_agent_info, summarize_agent_title};

/// Poll interval. `pty_agent_info` shells out to `ps` and (for Claude/Codex
/// panes) re-reads the agent's history file, so a slower cadence keeps the
/// process-table churn and file reads reasonable across many panes.
const POLL_INTERVAL_MS: u64 = 1500;

/// Mount-once component. Renders nothing -- exists only to own the `use_future`.
#[allow(non_snake_case)]
pub fn AgentInfoPoller() -> Element {
    let terminal_store = use_terminal_store();
    // Capture the registry synchronously at render top. `use_terminal_registry()`
    // is a Dioxus hook (wraps `use_context`); calling it inside the `use_future`
    // body (which runs after render) re-enters the hook list and panics at mount
    // with "hook list already borrowed". Clone the cheap `Rc`-backed handle into
    // the future instead.
    let terminal_registry = use_terminal_registry();
    let workspace = use_workspace_store();
    let ui_state = use_ui_store();

    // Track which (pane_id, session_id) pairs we have already summarized.
    let summarized_pairs: Signal<std::collections::HashSet<(String, String)>> =
        use_signal(std::collections::HashSet::new);

    use_future(move || {
        let mut terminal_store = terminal_store.clone();
        let workspace = workspace.clone();
        let mut summarized_pairs = summarized_pairs.clone();
        // Cheap `Rc`-bump clone of the render-top-captured registry.
        let registry = terminal_registry.clone();
        async move {
            loop {
                // Snapshot the pane ids of every space (not just the active
                // one) so a background space keeps its detection fresh too.
                let (pane_ids, pane_types) = {
                    let ws = workspace.read();
                    let ids: Vec<String> = ws
                        .spaces
                        .iter()
                        .flat_map(|s| s.panes.iter().map(|p| p.id.clone()))
                        .collect();
                    let types: std::collections::HashMap<
                        String,
                        crate::types::workspace::AgentType,
                    > = ws
                        .spaces
                        .iter()
                        .flat_map(|s| s.panes.iter().map(|p| (p.id.clone(), p.agent_type.clone())))
                        .collect();
                    (ids, types)
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

                            let sid = info.session_id.as_deref().unwrap_or_default();
                            let feature_enabled = ui_state.read().smart_pane_titles;
                            let raw = info.raw_prompt.as_deref().unwrap_or_default();
                            let prompt_ready = !sid.is_empty() && !raw.trim().is_empty();

                            // Detect session change and reset title state when
                            // the user starts a new agent run in this pane.
                            // Phase 2: writes the per-pane inner signal only — no
                            // whole-store generation bump, so other panes don't
                            // re-render on this pane's title-state reset.
                            {
                                if let Some(mut inner) = registry.write_session(pane_id) {
                                    let old_sid = inner.session_id.as_deref().unwrap_or_default();
                                    if old_sid != sid {
                                        inner.title_state =
                                            crate::utils::pane_label::TitleState::Idle;
                                        inner.generation = inner.generation.wrapping_add(1);
                                    }
                                }
                            }

                            // Only invoke LLM summarization for actual agent
                            // panes. Shells must never scrape global state.
                            let is_shell = matches!(
                                pane_types.get(pane_id),
                                Some(crate::types::workspace::AgentType::Shell)
                            );
                            if feature_enabled && prompt_ready && !is_shell {
                                let key = (pane_id.clone(), sid.to_string());
                                if !summarized_pairs.read().contains(&key) {
                                    summarized_pairs.write().insert(key);
                                    let raw_prompt = raw.to_string();
                                    let pane = pane_id.clone();

                                    // Idle -> Pending. The pill renders empty while waiting.
                                    // Phase 2: inner-signal only; no whole-store bump.
                                    {
                                        if let Some(mut inner) = registry.write_session(&pane) {
                                            inner.title_state =
                                                crate::utils::pane_label::TitleState::Pending;
                                            inner.generation = inner.generation.wrapping_add(1);
                                        }
                                    }

                                    // Clone the registry into the fire-and-forget closure so the
                                    // LLM result writes the inner signal. Phase 2: the sole write
                                    // path is the registry; the store is not touched here, so we
                                    // don't need a separate store clone.
                                    let registry_for_spawn = registry.clone();
                                    // Fire-and-forget. The backend command retries transient
                                    // failures internally, so this single await yields a final
                                    // result (title, "Sensitive prompt", or Err).
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let result = summarize_agent_title(&raw_prompt).await;
                                        // Write the LLM result into the per-pane inner signal.
                                        // If the pane was already closed (no signal), drop the
                                        // result — there's nothing left to title.
                                        let Some(mut inner) =
                                            registry_for_spawn.write_session(&pane)
                                        else {
                                            return;
                                        };
                                        match result {
                                            Ok(summary) => {
                                                let cleaned = summary.trim().to_string();
                                                web_sys::console::log_1(
                                                    &format!(
                                                        "[AgentInfoPoller] title for pane={}: {}",
                                                        pane, cleaned
                                                    )
                                                    .into(),
                                                );
                                                inner.title_state =
                                                    crate::utils::pane_label::TitleState::Done(
                                                        cleaned,
                                                    );
                                            }
                                            Err(e) => {
                                                web_sys::console::warn_1(
                                                    &format!(
                                                        "[AgentInfoPoller] title failed for pane={}: {:?}",
                                                        pane, e
                                                    )
                                                    .into(),
                                                );
                                                // Failed is terminal: the pill stays empty.
                                                inner.title_state =
                                                    crate::utils::pane_label::TitleState::Failed;
                                            }
                                        }
                                        inner.generation = inner.generation.wrapping_add(1);
                                    });
                                }
                            }
                            terminal_store.write().update_agent_info(
                                &registry,
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
