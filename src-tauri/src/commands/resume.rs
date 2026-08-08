use super::pty::{session_foreground_label, AGENT_FG_NAMES};
use crate::state::AppState;
// ---------------------------------------------------------------------------
// App-exit resume capture
// ---------------------------------------------------------------------------

/// Merge captured `pane_id -> resume_id` pairs directly into the persisted
/// `workspaces` JSON — the single source of truth the frontend loads on
/// startup. For each matching pane this sets `resume_id`, clears `resume_cmd`,
/// and resets `resume_dismissed = false` so the resume banner reappears on the
/// next launch via the normal workspace-load path (no separate transient key
/// for the frontend to reconcile, and no frontend startup changes).
///
/// Operates on `serde_json::Value` to avoid coupling the backend to the
/// frontend's `WorkspaceState`/`PaneConfig` Rust types. Returns the number of
/// panes updated. A missing/empty `workspaces` key (first run) yields `Ok(0)`.
pub(crate) fn merge_resume_ids_into_workspaces(
    store: &athena_store::KeyValueStore,
    ids: &std::collections::HashMap<String, String>,
    cmds: &std::collections::HashMap<String, String>,
) -> Result<usize, String> {
    if ids.is_empty() {
        return Ok(0);
    }
    let json = match store.get::<String>("workspaces") {
        Ok(Some(j)) if !j.trim().is_empty() => j,
        Ok(_) => return Ok(0), // no workspace persisted yet — nothing to merge into
        Err(e) => return Err(e.to_string()),
    };
    let mut root: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    let mut updated = 0usize;
    if let Some(spaces) = root.get_mut("spaces").and_then(|v| v.as_array_mut()) {
        for space in spaces.iter_mut() {
            let Some(panes) = space.get_mut("panes").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for pane in panes.iter_mut() {
                let pane_id = pane
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let Some(pane_id) = pane_id else { continue };
                let Some(resume_id) = ids.get(&pane_id) else {
                    continue;
                };
                if let Some(obj) = pane.as_object_mut() {
                    obj.insert(
                        "resume_id".into(),
                        serde_json::Value::String(resume_id.clone()),
                    );
                    // Prefer a captured resume_cmd ; fall back to one built from
                    // the resume_id so Shell panes (whose agent_type can't be
                    // synthesized) still get a displayable command.
                    let resume_cmd = cmds
                        .get(&pane_id)
                        .cloned()
                        .unwrap_or_else(|| resume_id.clone());
                    obj.insert("resume_cmd".into(), serde_json::Value::String(resume_cmd));
                    obj.insert("resume_dismissed".into(), serde_json::Value::Bool(false));
                    updated += 1;
                }
            }
        }
    }

    if updated > 0 {
        let out = serde_json::to_string(&root).map_err(|e| e.to_string())?;
        store
            .set_sync("workspaces", &out)
            .map_err(|e| e.to_string())?;
    }
    Ok(updated)
}

/// App-exit resume capture, invoked from `RunEvent::Exit` (the event macOS
/// Cmd+Q reliably fires). Types `/exit` into every live PTY so agents (Claude,
/// Codex, …) exit gracefully and print their `<cli> --resume <id>` line — the
/// same line the live frontend scanner catches during a manual `/exit`. We then
/// scan each pane's output buffer for that id and merge it straight into the
/// persisted `workspaces` state, so the banner reappears on next launch. Plain
/// shells just echo a harmless "not found" and yield no match.
///
/// Returns the number of panes whose resume id was captured + persisted.
///
/// Concurrency: the caller runs this on a DEDICATED runtime/thread during
/// `RunEvent::Exit`, while the shared runtime's `pty_read_loop` tasks keep
/// feeding the output buffer with the agents' exit output (see
/// `capture_resume_on_exit` in main.rs).
pub async fn capture_resume_ids_on_exit(state: &AppState, wait_ms: u64) -> usize {
    let all_sessions = {
        let sm = state.session_manager.lock().await;
        sm.list_sessions().await
    };
    if all_sessions.is_empty() {
        return 0;
    }

    // Classify each session's foreground process so we only nudge *agents*
    // with `/exit`. Plain shells never produce a resume id, so sending to
    // them would waste the entire wait budget. The classification costs a `ps`
    // per session, but that is fast compared to the 4 s wait budget.
    let agent_sessions: Vec<String> = {
        let sm = state.session_manager.lock().await;
        let mut agents = Vec::new();
        for id in &all_sessions {
            match sm.get_session(id).await {
                Some(s) => {
                    let label = session_foreground_label(&s).await;
                    if AGENT_FG_NAMES.contains(&label.as_str()) {
                        agents.push(id.clone());
                    }
                }
                None => continue,
            }
        }
        agents
    };

    if agent_sessions.is_empty() {
        log::info!(
            "capture_resume_ids_on_exit: {} live session(s), none are agents — nothing to do",
            all_sessions.len()
        );
        return 0;
    }

    log::info!(
        "capture_resume_ids_on_exit: {} live session(s), {} agent(s) — nudging with /exit",
        all_sessions.len(),
        agent_sessions.len()
    );

    // Send `/exit` + Enter to every agent PTY.
    {
        let sm = state.session_manager.lock().await;
        for id in &agent_sessions {
            if let Err(e) = sm.write(id, b"/exit\r").await {
                log::warn!("capture_resume_ids_on_exit: write to {} failed: {}", id, e);
            }
        }
    }

    // Poll the output buffer until every agent pane yields a resume id or we
    // hit the deadline. The PTY read loops populate the buffer on the shared
    // runtime.
    let step_ms = 150u64;
    let mut elapsed = 0u64;
    let mut found: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut found_cmds: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    while elapsed < wait_ms && found.len() < agent_sessions.len() {
        tokio::time::sleep(std::time::Duration::from_millis(step_ms)).await;
        elapsed += step_ms;
        for id in &agent_sessions {
            if found.contains_key(id) {
                continue;
            }
            let lines = state.output_buffer.get_output(id, None);
            if lines.is_empty() {
                continue;
            }
            let text: String = lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if let Some((prefix, rid)) = athena_core::resume_scanner::scan_text_for_resume_id(&text)
            {
                log::info!(
                    "capture_resume_ids_on_exit: captured resume id for pane {}",
                    id
                );
                let cmd = format!("{} {}", prefix, rid);
                found_cmds.insert(id.clone(), cmd);
                found.insert(id.clone(), rid);
            }
        }
    }

    if found.is_empty() {
        log::info!("capture_resume_ids_on_exit: no resume ids captured");
        return 0;
    }

    match merge_resume_ids_into_workspaces(&state.store, &found, &found_cmds) {
        Ok(n) => {
            if let Err(e) = state.store.flush_if_dirty().await {
                log::error!("capture_resume_ids_on_exit: KV flush failed: {}", e);
            }
            log::info!(
                "capture_resume_ids_on_exit: merged {} resume id(s) into {} pane(s)",
                found.len(),
                n
            );
            n
        }
        Err(e) => {
            log::error!(
                "capture_resume_ids_on_exit: merge into workspaces failed: {}",
                e
            );
            0
        }
    }
}
