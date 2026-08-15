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
    log::info!(
        "[resume-debug] merge requested: {} pane id(s), {} command(s)",
        ids.len(),
        cmds.len()
    );
    if ids.is_empty() {
        log::info!("[resume-debug] merge skipped: no captured ids");
        return Ok(0);
    }
    let json = match store.get::<String>("workspaces") {
        Ok(Some(j)) if !j.trim().is_empty() => {
            log::info!(
                "[resume-debug] merge loaded workspaces store ({} bytes)",
                j.len()
            );
            j
        }
        Ok(_) => {
            log::warn!("[resume-debug] merge found no persisted workspaces key");
            return Ok(0);
        }
        Err(e) => {
            log::error!("[resume-debug] merge could not read workspaces store: {e}");
            return Err(e.to_string());
        }
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
                log::info!(
                    "[resume-debug] merge matched pane={} resume_id_len={} command_present={}",
                    pane_id,
                    resume_id.len(),
                    cmds.contains_key(&pane_id)
                );
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
        log::info!("[resume-debug] merge persisted {} pane(s)", updated);
    } else {
        log::warn!("[resume-debug] merge captured ids but matched no persisted panes");
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
    log::info!("[resume-debug] capture begin wait_ms={wait_ms}");
    let all_sessions = {
        let sm = state.session_manager.lock().await;
        sm.list_sessions().await
    };
    log::info!(
        "[resume-debug] capture discovered {} live PTY session(s): {:?}",
        all_sessions.len(),
        all_sessions
    );
    if all_sessions.is_empty() {
        log::warn!("[resume-debug] capture stopped: no live PTY sessions");
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
                    let is_agent = AGENT_FG_NAMES.contains(&label.as_str());
                    log::info!(
                        "[resume-debug] classify pane={} foreground={} is_agent={}",
                        id,
                        label,
                        is_agent
                    );
                    if is_agent {
                        agents.push(id.clone());
                    }
                }
                None => {
                    log::warn!(
                        "[resume-debug] classify pane={} missing from session manager",
                        id
                    );
                    continue;
                }
            }
        }
        agents
    };

    if agent_sessions.is_empty() {
        log::info!(
            "[resume-debug] capture stopped: {} live session(s), none classified as agents",
            all_sessions.len()
        );
        return 0;
    }

    log::info!(
        "[resume-debug] capture nudging {} agent pane(s) with /exit",
        agent_sessions.len()
    );

    // Send `/exit` + Enter to every agent PTY.
    {
        let sm = state.session_manager.lock().await;
        for id in &agent_sessions {
            match sm.write(id, b"/exit\r").await {
                Ok(bytes) => log::info!("[resume-debug] sent /exit to pane={} bytes={bytes}", id),
                Err(e) => log::warn!("[resume-debug] /exit write failed pane={} error={e}", id),
            }
        }
    }

    // Poll the output buffer until every agent pane yields a resume id or we
    // hit the deadline. The PTY read loops populate the buffer on the shared
    // runtime.
    let step_ms = 150u64;
    let started = tokio::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(wait_ms);
    let mut found: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut found_cmds: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut logged_output = std::collections::HashSet::new();
    let mut logged_hint_output = std::collections::HashSet::new();
    let mut poll_count = 0u32;
    while tokio::time::Instant::now() < deadline && found.len() < agent_sessions.len() {
        poll_count += 1;
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        tokio::time::sleep(std::time::Duration::from_millis(step_ms).min(remaining)).await;
        for id in &agent_sessions {
            if found.contains_key(id) {
                continue;
            }
            let lines = state.output_buffer.get_output(id, None);
            if lines.is_empty() {
                continue;
            }
            if logged_output.insert(id.clone()) {
                log::info!(
                    "[resume-debug] output observed pane={} lines={} chars={}",
                    id,
                    lines.len(),
                    lines.iter().map(|line| line.text.len()).sum::<usize>()
                );
            }
            let text: String = lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let lower = text.to_ascii_lowercase();
            if (lower.contains("freebuff") || lower.contains("omp"))
                && (lower.contains("resume") || lower.contains("continue"))
                && logged_hint_output.insert(id.clone())
            {
                log::info!(
                    "[resume-debug] hint-like output observed pane={} chars={} has_freebuff={} has_omp={} has_resume={} has_continue={}",
                    id,
                    text.len(),
                    lower.contains("freebuff"),
                    lower.contains("omp"),
                    lower.contains("resume"),
                    lower.contains("continue")
                );
            }
            if let Some((prefix, rid)) = athena_core::resume_scanner::scan_text_for_resume_id(&text)
            {
                log::info!(
                    "[resume-debug] scanner matched pane={} prefix={} resume_id_len={} output_chars={}",
                    id,
                    prefix,
                    rid.len(),
                    text.len()
                );
                let cmd = format!("{} {}", prefix, rid);
                found_cmds.insert(id.clone(), cmd);
                found.insert(id.clone(), rid);
            }
        }
    }

    if found.is_empty() {
        log::warn!(
            "[resume-debug] scanner found no resume ids after {} poll(s); output_seen_for={:?} hint_like_output_for={:?}",
            poll_count,
            logged_output,
            logged_hint_output
        );
        return 0;
    }

    log::info!(
        "[resume-debug] scanner captured {} of {} agent pane(s)",
        found.len(),
        agent_sessions.len()
    );

    match merge_resume_ids_into_workspaces(&state.store, &found, &found_cmds) {
        Ok(n) => {
            if let Err(e) = state.store.flush_if_dirty().await {
                log::error!("[resume-debug] KV flush failed: {}", e);
            }
            log::info!(
                "[resume-debug] capture merge completed: {} resume id(s) into {} pane(s)",
                found.len(),
                n
            );
            n
        }
        Err(e) => {
            log::error!("[resume-debug] capture merge into workspaces failed: {}", e);
            0
        }
    }
}
