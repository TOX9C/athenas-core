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
/// When `skip_unchanged` is true (the heartbeat exit-capture path), a pane
/// whose stored `resume_id` already equals the captured id is left untouched:
/// no write, no `resume_dismissed` reset, no event churn. The app-exit path
/// passes false so a fresh capture always resets the dismissed flag and the
/// banner reappears on next launch.
///
/// Operates on `serde_json::Value` to avoid coupling the backend to the
/// frontend's `WorkspaceState`/`PaneConfig` Rust types. Returns the new
/// serialized `workspaces` JSON when at least one pane was updated, so callers
/// (e.g. the heartbeat) can broadcast `workspace:changed`. A missing/empty
/// `workspaces` key (first run) yields `Ok(None)`.
pub(crate) fn merge_resume_ids_into_workspaces(
    store: &athena_store::KeyValueStore,
    ids: &std::collections::HashMap<String, String>,
    cmds: &std::collections::HashMap<String, String>,
    skip_unchanged: bool,
) -> Result<Option<String>, String> {
    log::info!(
        "merge requested: {} pane id(s), {} command(s), skip_unchanged={}",
        ids.len(),
        cmds.len(),
        skip_unchanged
    );
    if ids.is_empty() {
        log::info!("merge skipped: no captured ids");
        return Ok(None);
    }
    let json = match store.get::<String>("workspaces") {
        Ok(Some(j)) if !j.trim().is_empty() => {
            log::info!(
                "merge loaded workspaces store ({} bytes)",
                j.len()
            );
            j
        }
        Ok(_) => {
            log::warn!("merge found no persisted workspaces key");
            return Ok(None);
        }
        Err(e) => {
            log::error!("merge could not read workspaces store: {e}");
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
                if skip_unchanged
                    && pane.get("resume_id").and_then(|v| v.as_str()) == Some(resume_id.as_str())
                {
                    log::info!(
                        "merge skipped unchanged pane={} (same resume id)",
                        pane_id
                    );
                    continue;
                }
                log::info!(
                    "merge matched pane={} resume_id_len={} command_present={}",
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
        log::info!("merge persisted {} pane(s)", updated);
        Ok(Some(out))
    } else {
        log::warn!("merge captured ids but matched no persisted panes");
        Ok(None)
    }
}

/// Scan one pane's accumulated PTY output for the newest `<cli> --resume <id>`
/// (or harness continuation) hint. Returns `(prefix, id)` where `prefix` is the
/// CLI prefix without trailing space — the same shape
/// [`ResumeScanner::feed`](athena_core::resume_scanner::ResumeScanner::feed)
/// yields. Shared by the app-exit capture, the post-shutdown rescan, and the
/// heartbeat's per-session exit detection.
///
/// Feeds the pane line-by-line through a stateful rolling scanner instead of
/// joining the whole buffer into one `String` (plus an ANSI-stripped copy)
/// per call: memory stays bounded to the scanner's rolling window while the
/// whole buffer is still covered, because each feed rescans the accumulated
/// tail.
pub(crate) fn scan_pane_for_resume_id(
    output_buffer: &athena_core::output_buffer::OutputBuffer,
    pane_id: &str,
) -> Option<(String, String)> {
    let lines = output_buffer.get_output(pane_id, None);
    if lines.is_empty() {
        return None;
    }
    let mut scanner = athena_core::resume_scanner::ResumeScanner::new();
    let mut newest = None;
    for line in &lines {
        // Restore the line separator the buffer splits away: without it, the
        // tail of one line fuses with the head of the next and the id
        // scanner absorbs following text into a trailing alphanumeric id.
        let mut text = line.text.clone();
        text.push('\n');
        if let Some(hit) = scanner.feed(&text) {
            newest = Some(hit);
        }
    }
    newest
}

/// Merge captured resume ids into the persisted workspaces store and flush.
/// Shared tail of the app-exit capture and post-shutdown rescan. Returns the
/// number of captured ids that were merged (best-effort: merge/flush failures
/// are logged, not propagated — shutdown must not stall on persistence).
pub(crate) async fn merge_captured_and_flush(
    state: &AppState,
    found: std::collections::HashMap<String, String>,
    found_cmds: std::collections::HashMap<String, String>,
) -> usize {
    match merge_resume_ids_into_workspaces(&state.store, &found, &found_cmds, false) {
        Ok(outcome) => {
            if let Err(e) = state.store.flush_if_dirty().await {
                log::error!("KV flush failed: {}", e);
            }
            let n = if outcome.is_some() { found.len() } else { 0 };
            log::info!(
                "capture merge completed: {} resume id(s) into pane(s)",
                n
            );
            n
        }
        Err(e) => {
            log::error!("capture merge into workspaces failed: {}", e);
            0
        }
    }
}

/// App-exit resume capture, invoked from `RunEvent::Exit` (the event macOS
/// Cmd+Q reliably fires). Types `/exit` into every live PTY so agents (Claude,
/// Codex, …) exit gracefully and print their `<cli> --resume <id>` line — the
/// same line the live frontend scanner catches during a manual `/exit`. We then
/// scan each pane's output buffer for that id and merge it straight into the
/// persisted `workspaces` state, so the banner reappears on next launch. Plain
/// shells just echo a harmless "not found" and yield no match.
///
/// Only *nudging* is gated on agent classification. **Scanning covers every
/// session's output buffer**: an agent the user already exited before quitting
/// shows a shell foreground and would otherwise be skipped even though its
/// resume line is sitting in the buffer (scanning is a pure parse — free).
///
/// Returns `(captured_count, all_session_ids)`; the ids snapshot lets the
/// caller rescan the same buffers after `shutdown_all`'s SIGINT phase
/// (harnesses that only print their resume hint on Ctrl+C).
///
/// Concurrency: the caller runs this on a DEDICATED runtime/thread during
/// `RunEvent::Exit`, while the shared runtime's `pty_read_loop` tasks keep
/// feeding the output buffer with the agents' exit output (see
/// `capture_resume_on_exit` in main.rs).
pub async fn capture_resume_ids_on_exit(state: &AppState, wait_ms: u64) -> (usize, Vec<String>) {
    // Stall-guard budget: wait_ms is the inner POLL deadline (clock starts at
    // inner entry, before classification). Classification (`ps` per session)
    // and the final merge/flush happen outside that window, so the outer
    // timeout needs headroom or it fires right as a fully-found poll tries to
    // merge — discarding every captured id and the session snapshot. 2500 ms
    // of slack makes this timeout fire only on a true stall.
    let budget = std::time::Duration::from_millis(wait_ms.max(1) + 2500);
    match tokio::time::timeout(budget, capture_resume_ids_on_exit_inner(state, wait_ms)).await {
        Ok(result) => result,
        Err(_) => {
            log::warn!(
                "capture timed out after {}ms; abandoning without blocking shutdown",
                budget.as_millis()
            );
            (0, Vec::new())
        }
    }
}

async fn capture_resume_ids_on_exit_inner(state: &AppState, wait_ms: u64) -> (usize, Vec<String>) {
    log::info!("capture begin wait_ms={wait_ms}");
    // The poll deadline starts HERE, before classification: an n-pane `ps`
    // pass must eat into the same budget as the poll, or a slow classify
    // collides with the outer stall-guard timeout.
    let started = tokio::time::Instant::now();
    let all_sessions = {
        let sm = state.session_manager.lock().await;
        sm.list_sessions().await
    };
    log::info!(
        "capture discovered {} live PTY session(s): {:?}",
        all_sessions.len(),
        all_sessions
    );
    if all_sessions.is_empty() {
        log::warn!("capture stopped: no live PTY sessions");
        return (0, Vec::new());
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
                        "classify pane={} foreground={} is_agent={}",
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
                        "classify pane={} missing from session manager",
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
            "no panes classified as agents ({} live session(s)); skipping /exit nudge but still scanning buffers",
            all_sessions.len()
        );
    } else {
        log::info!(
            "capture nudging {} agent pane(s) with /exit",
            agent_sessions.len()
        );

        // Send `/exit` + Enter to every agent PTY.
        let sm = state.session_manager.lock().await;
        for id in &agent_sessions {
            match sm.write(id, b"/exit\r").await {
                Ok(bytes) => log::info!("sent /exit to pane={} bytes={bytes}", id),
                Err(e) => log::warn!("/exit write failed pane={} error={e}", id),
            }
        }
    }

    // Scan-first poll: one pass even when nothing was nudged (deadline=now)
    // so buffers holding a resume line from an already-exited agent are still
    // captured. Every session is scanned each pass — classification only
    // gates the `/exit` nudge — but COMPLETION is judged on the nudged agent
    // panes alone: plain shells never yield a hint, so requiring every
    // session would burn the whole budget for nothing.
    let step_ms = 150u64;
    let deadline = if agent_sessions.is_empty() {
        started
    } else {
        started + std::time::Duration::from_millis(wait_ms)
    };
    let mut found: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut found_cmds: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut logged_output = std::collections::HashSet::new();
    let mut logged_hint_output = std::collections::HashSet::new();
    let mut poll_count = 0u32;
    // Rolling per-pane scanners + incremental line cursors: each poll feeds
    // only the lines appended since the previous poll instead of re-joining,
    // re-stripping, and re-lowercasing the entire pane buffer per exit-check.
    // The initial pass still covers the full existing buffer (line-by-line,
    // so the scanner's 1 KB rolling window slides over all of it).
    let mut scanners: std::collections::HashMap<String, athena_core::resume_scanner::ResumeScanner> =
        std::collections::HashMap::new();
    let mut last_line: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    loop {
        poll_count += 1;
        if poll_count > 1 {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(step_ms).min(remaining)).await;
        }
        for id in &all_sessions {
            if found.contains_key(id) {
                continue;
            }
            let cursor = last_line.get(id).copied().unwrap_or(0);
            let opts = athena_core::output_buffer::GetOutputOptions {
                limit: None,
                offset: None,
                since_line: Some(cursor),
                since_time: None,
                raw: None,
            };
            let lines = state.output_buffer.get_output(id, Some(&opts));
            if lines.is_empty() {
                continue;
            }
            let mut newest_line = cursor;
            let mut scanned_chars = 0usize;
            let scanner = scanners.entry(id.clone()).or_default();
            let mut matched = None;
            for line in &lines {
                newest_line = newest_line.max(line.line_num);
                scanned_chars += line.text.len();
                // Same line-boundary fix as scan_pane_for_resume_id.
                let mut text = line.text.clone();
                text.push('\n');
                if let Some(hit) = scanner.feed(&text) {
                    matched = Some(hit);
                }
            }
            last_line.insert(id.clone(), newest_line);
            if logged_output.insert(id.clone()) {
                log::info!(
                    "output observed pane={} new_lines={} new_chars={}",
                    id,
                    lines.len(),
                    scanned_chars
                );
            }
            // Hint-like-output debug: needs the lowercase of the newly read
            // text — computed only when the log line is actually reachable.
            if log::log_enabled!(log::Level::Info) {
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
                        "hint-like output observed pane={} chars={} has_freebuff={} has_omp={} has_resume={} has_continue={}",
                        id,
                        text.len(),
                        lower.contains("freebuff"),
                        lower.contains("omp"),
                        lower.contains("resume"),
                        lower.contains("continue")
                    );
                }
            }
            if let Some((prefix, rid)) = matched {
                log::info!(
                    "scanner matched pane={} prefix={} resume_id_len={} new_chars={}",
                    id,
                    prefix,
                    rid.len(),
                    scanned_chars
                );
                let cmd = format!("{} {}", prefix, rid);
                found_cmds.insert(id.clone(), cmd);
                found.insert(id.clone(), rid);
            }
        }
        if agent_sessions.iter().all(|id| found.contains_key(id)) {
            break;
        }
    }

    if found.is_empty() {
        log::warn!(
            "scanner found no resume ids after {} poll(s); output_seen_for={:?} hint_like_output_for={:?}",
            poll_count,
            logged_output,
            logged_hint_output
        );
        return (0, all_sessions);
    }

    log::info!(
        "scanner captured {} of {} live session(s) ({} agent pane(s) nudged)",
        found.len(),
        all_sessions.len(),
        agent_sessions.len()
    );

    let n = merge_captured_and_flush(state, found, found_cmds).await;
    (n, all_sessions)
}
