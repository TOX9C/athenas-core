use super::{validate_path_exists, CommandError};
use crate::state::AppState;
use base64::Engine;
use std::sync::Arc;
use tauri::{Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

// ── PTY spawn/write validation helpers ───────────────────────────────────────
//
// The IPC boundary is the entire frontend renderer. A compromised or XSSed
// renderer (note: CSP still permits `unsafe-eval`) could otherwise spawn an
// arbitrary binary rooted at `~/.ssh` or `/`, or paste unbounded data into a
// live PTY. These helpers gate the shell binary, the working directory, and
// payload sizes.

/// Maximum bytes accepted by `pty_write` / `pty_spawn_agent`'s `agent_cmd`.
/// Matches the cap used elsewhere for raw data payloads.
const MAX_PTY_DATA_BYTES: usize = 1024 * 1024; // 1 MB

/// Maximum length of a PTY session id. Generous; ids are caller-chosen.
const MAX_SESSION_ID_LEN: usize = 256;

/// Validate a shell binary path for PTY spawning.
///
/// Allowed if the canonicalized path lives under `/bin` or `/usr/bin`, or if it
/// matches the invoking user's `$SHELL` after canonicalization. This stops a
/// renderer from spawning `/usr/sbin/installer`, a homebrew binary, or an
/// arbitrary executable while still permitting standard shells (bash, zsh,
/// sh, fish when in /usr/bin).
fn validate_shell(shell: &str) -> Result<std::path::PathBuf, String> {
    if shell.is_empty() {
        return Err("shell path is empty".to_string());
    }
    let p = std::path::Path::new(shell);

    // Canonicalize the provided shell for comparison.
    let canon = p
        .canonicalize()
        .map_err(|e| format!("shell binary not accessible: {}", e))?;

    // Check $SHELL after canonicalization so an attacker can't set
    // SHELL=/tmp/evil and then invoke `pty_spawn` with that path.
    if let Ok(user_shell) = std::env::var("SHELL") {
        let user_canon = std::path::Path::new(&user_shell)
            .canonicalize()
            .map_err(|e| format!("$SHELL not accessible: {}", e))?;
        if canon == user_canon {
            return Ok(canon);
        }
    }

    // Canonicalize and require the binary to live under a system bin dir.
    // Using canonicalize (not lexical check) so symlinks are resolved: a
    // symlink in /bin pointing to /Users/x/evil is followed and then rejected
    // because the target isn't under /bin or /usr/bin.
    let allowed_ancestors = [
        std::path::Path::new("/bin"),
        std::path::Path::new("/usr/bin"),
    ];
    let ok = allowed_ancestors
        .iter()
        .any(|allowed| canon.starts_with(allowed));
    if !ok {
        return Err(format!(
            "shell binary '{}' is outside the allowed system directories (/bin, /usr/bin) and does not match $SHELL",
            shell
        ));
    }
    Ok(canon)
}

/// Validate a working directory for PTY spawning.
///
/// The directory must exist, be a directory, and be inside the sandbox (the
/// app project root ∪ trusted workspace roots). This stops a renderer from
/// spawning a shell rooted at `/`, `~/.ssh`, or an arbitrary path the user
/// never opted in to, while permitting every directory the user deliberately
/// turned into a Space.
fn validate_cwd(
    store: &athena_store::KeyValueStore,
    cwd: &str,
) -> Result<std::path::PathBuf, CommandError> {
    if cwd.is_empty() {
        return Err(CommandError::Internal("cwd is empty".to_string()));
    }
    let validated = validate_path_exists(store, std::path::Path::new(cwd))?;
    if !validated.is_dir() {
        return Err(CommandError::Internal("cwd is not a directory".to_string()));
    }
    Ok(validated)
}

/// Validate a PTY session id: non-empty, bounded length, no control chars.
fn validate_session_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("session id is empty".to_string());
    }
    if id.len() > MAX_SESSION_ID_LEN {
        return Err(format!(
            "session id too long: {} > {}",
            id.len(),
            MAX_SESSION_ID_LEN
        ));
    }
    if id.chars().any(|c| c.is_control()) {
        return Err("session id contains control characters".to_string());
    }
    Ok(())
}

/// Validate a raw-data payload size for PTY writes.
fn validate_data_size(data: &[u8], label: &str) -> Result<(), String> {
    if data.len() > MAX_PTY_DATA_BYTES {
        return Err(format!(
            "{} too large: {} > {}",
            label,
            data.len(),
            MAX_PTY_DATA_BYTES
        ));
    }
    Ok(())
}

/// Return the generated shell hooks when the selected shell supports OSC 633.
/// The terminal crate loads this through the shell's startup mechanism, never by
/// typing the script into the interactive PTY (which would echo the definitions).
fn shell_integration_script(shell: &str) -> Option<String> {
    athena_core::shell_integration::get_shell_integration_script(shell).ok()
}

/// Return the default shell path for the current platform.
#[tauri::command]
pub fn pty_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "powershell.exe".to_string()
        } else {
            "/bin/zsh".to_string()
        }
    })
}

// ── PTY commands ─────────────────────────────────────────────────────────────

/// Spawn a new PTY session with the given ID, working directory, and shell.
/// After spawning, starts a background tokio task that reads PTY output
/// and emits `terminal:data` events to the frontend.
#[tauri::command]
// Keep the PTY lifecycle handshake explicit at this Tauri wire boundary.
#[allow(clippy::too_many_arguments)]
pub async fn pty_spawn(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    cols: Option<u16>,
    rows: Option<u16>,
    start_paused: Option<bool>,
    listener_owner: Option<String>,
) -> Result<(), String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);
    log::info!(
        "pty_spawn requested: session={} cols={} rows={}",
        id,
        cols,
        rows
    );
    // Validate caller-supplied values before touching the session manager.
    validate_session_id(&id).inspect_err(|e| log::warn!("pty_spawn rejected (bad id): {}", e))?;
    let validated_shell = validate_shell(&shell)
        .inspect_err(|_| log::warn!("pty_spawn rejected: invalid shell input"))?;
    let validated_cwd = validate_cwd(&state.store, &cwd).map_err(|e| {
        log::warn!("pty_spawn rejected: invalid workspace cwd");
        e.to_string()
    })?;
    let shell_str = validated_shell.to_string_lossy().to_string();
    let cwd_str = validated_cwd.to_string_lossy().to_string();

    let integration_script = shell_integration_script(&shell_str);
    let session_manager = state.session_manager.lock().await;
    let session_result = session_manager
        .spawn_with_startup_script(
            id.clone(),
            &shell_str,
            &cwd_str,
            cols,
            rows,
            integration_script.as_deref(),
        )
        .await;
    drop(session_manager);

    match session_result {
        Ok(session) => {
            let _session_id = id.clone();
            let app_handle = state.app_handle.lock().clone();

            // SessionManager::spawn returns an existing Arc for duplicate IDs.
            // Start exactly one reader for that session; a second reader would
            // race the first on the same PTY and duplicate/split shell echoes.
            // A newly claimed reader may start paused so a mobile/xterm mount
            // can install its raw listener before the first screen is emitted.
            let claimed_reader = session.try_claim_read_loop();
            if claimed_reader && start_paused.unwrap_or(false) {
                session.begin_startup_pause(listener_owner);
            }
            if let Some(handle) = app_handle {
                if claimed_reader {
                    let session_id_for_loop = id.clone();
                    let output_buffer = std::sync::Arc::clone(&state.output_buffer);
                    let tracker = std::sync::Arc::clone(&state.agent_activity);
                    tokio::spawn(async move {
                        pty_read_loop(
                            handle,
                            session_id_for_loop,
                            session,
                            output_buffer,
                            Some(tracker),
                        )
                        .await;
                    });
                } else {
                    log::debug!("pty_spawn: read loop already running for session {}", id);
                }
            } else if claimed_reader {
                // There is no reader to release this claim. A later spawn with
                // an app handle must be allowed to start the loop.
                session.release_read_loop();
            }

            log::info!(
                "PTY session spawned: session={} cols={} rows={}",
                id,
                cols,
                rows
            );
            Ok(())
        }
        Err(e) => {
            log::error!(
                "Failed to spawn PTY session: session={} cols={} rows={}",
                id,
                cols,
                rows
            );
            Err(e.to_string())
        }
    }
}

/// Background task that reads PTY output and emits Tauri events.
///
/// Fans out to two parallel event streams:
/// - `pty:raw` — base64-encoded raw PTY bytes, consumed by the xterm.js
///   frontend (which has its own ANSI parser). Emitted in coalesced
///   batches (one event per flush) to reduce per-event overhead and
///   give the frontend larger, more stable chunks to render.
/// - `terminal:data` — parsed cell deltas, consumed by the legacy
///   cell-grid frontend. Emitted only when the grid actually changed.
pub(crate) async fn pty_read_loop(
    app_handle: tauri::AppHandle,
    session_id: String,
    session: std::sync::Arc<athena_terminal::session::TerminalSession>,
    output_buffer: std::sync::Arc<athena_core::output_buffer::OutputBuffer>,
    agent_activity: Option<std::sync::Arc<athena_core::agent_activity::AgentActivityTracker>>,
) {
    log::info!("pty_read_loop[{}]: starting", session_id);
    let registration_generation = agent_activity.as_ref().map(|tracker| {
        // A pane id may be reused after a previous PTY closed. Explicitly
        // register at the new read-loop boundary so the retired-pane guard is
        // cleared before heartbeat/output processing begins.
        tracker.register_pane_with_generation(&session_id)
    });
    let mut did_emit_ready = false;

    // Per-session OSC 633 parser + command tracker. Supplementary signal:
    // it produces events only when the shell-integration scripts are active
    // in the pane; the heartbeat's process-lifecycle detection is the
    // primary completion signal.
    let mut shell_parser = athena_core::shell_integration::Osc633Parser::new();
    let mut shell_tracker = athena_core::shell_integration::CommandTracker::new();

    // 16 KB read buffer — reduces per-read syscall overhead while staying
    // below typical kernel pagecache sizes for good latency.
    let mut read_buf = vec![0u8; 16 * 1024];

    // Coalescing buffer for `pty:raw` PTY output. Pre-allocate to 32 KB
    // to avoid reallocation churn during active output.
    let mut coalesce_buf: Vec<u8> = Vec::with_capacity(32 * 1024);
    // Reusable base64 output buffer. `flush_pty_raw` can fire up to 125×/sec
    // per session; without reuse each flush allocates a fresh base64 String.
    // We instead encode into this buffer with `encode_slice` (zero-alloc) and
    // grow it lazily — capacity is a monotonic high-water mark, so steady-state
    // flushes hit zero reallocs.
    let mut encode_buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    // Reusable JSON event string. Same rationale — avoids a per-flush String
    // + serde_json::Value tree allocation. Cleared (not freed) between flushes.
    let mut raw_event_buf: String = String::with_capacity(64 * 1024);
    // Coalesce-flush threshold (32 KB). Above this size we emit immediately to
    // avoid unbounded memory growth; below it we keep coalescing for the next
    // rate-limited flush. (The former separate 1 MB cap was removed: the two
    // thresholds triggered the identical flush call, so the lower one governed.)

    // Backstop cap while `raw_paused` is true. All threshold flushes (incl.
    // the 32 KB coalesce-flush above) are skipped while paused,
    // so without this cap a chatty shell (`yes`, `find /`) during a slow or
    // stalled remount would grow `coalesce_buf` without bound. 4 MB is ~40k
    // lines — far beyond any real swap window (<300 ms, typically <2 KB). If
    // a remount stalls long enough to hit this, we drop the oldest half
    // rather than keep growing: a paused remount means no listener is ready
    // anyway, and the alternative (holding 4 MB+ per pane) risks OOM across
    // many panes. Drop-oldest preserves the most recent output the remount
    // will actually surface.
    const PAUSED_MAX_COALESCE_SIZE: usize = 4 * 1024 * 1024; // 4 MB

    // Tracks `raw_paused` across loop iterations so the read loop detects the
    // true → false transition and flushes the accumulated buffer on that
    // iteration. This makes the unpause self-healing: ANY caller that clears
    // `raw_paused` (the frontend remount, or `pty_attach_listener`) gets the
    // burst automatically, decoupling the flush from who cleared the flag.
    let mut was_paused = session
        .raw_paused
        .load(std::sync::atomic::Ordering::Relaxed);

    // 8 ms flush interval — balances latency with batching efficiency.
    let mut flush_interval = tokio::time::interval(tokio::time::Duration::from_millis(8));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    /// Flush accumulated raw PTY bytes as a single `pty:raw` event.
    ///
    /// `encode_buf` and `raw_event_buf` are reusable scratch buffers threaded
    /// in by the caller so this hot path (up to 125×/sec per session) performs
    /// zero allocations after warmup — previously each flush allocated a fresh
    /// base64 `String`, a full `serde_json::Value` tree, and a fresh JSON
    /// `String`.
    fn flush_pty_raw(
        coalesce_buf: &mut Vec<u8>,
        encode_buf: &mut Vec<u8>,
        raw_event_buf: &mut String,
        app_handle: &tauri::AppHandle,
        session_id: &str,
    ) {
        if coalesce_buf.is_empty() {
            return;
        }
        // NOTE: Tauri's `emit` serializes payloads to JSON before crossing
        // the IPC boundary. JSON cannot natively carry raw byte arrays,
        // so we must base64-encode. Passing `Vec<u8>` directly would only
        // result in an array-of-numbers JSON payload (more expensive to
        // parse and emit than a compact base64 string). True ArrayBuffer
        // transfer (ZeroCopy) over Tauri's `postMessage` IPC is not
        // supported by `tauri::Emitter::emit`, so base64 is the optimal
        // serialization for this event type.
        //
        // Encode directly into `encode_buf` (zero-alloc after warmup). base64
        // expands input by ~4/3, so ceil(len/3)*4 covers any output. `resize`
        // grows capacity as needed; the buffer never shrinks, so steady-state
        // flushes (hot path) hit zero reallocs.
        encode_buf.clear();
        let cap = (coalesce_buf.len() / 3 + 1) * 4;
        encode_buf.resize(cap, 0);
        let len = match base64::engine::general_purpose::STANDARD
            .encode_slice(coalesce_buf.as_slice(), encode_buf.as_mut_slice())
        {
            Ok(n) => n,
            // Unreachable given the cap sizing above, but never drop PTY
            // output — fall back to a plain encode.
            Err(_) => {
                let fallback =
                    base64::engine::general_purpose::STANDARD.encode(coalesce_buf.as_slice());
                encode_buf.clear();
                encode_buf.extend_from_slice(fallback.as_bytes());
                encode_buf.len()
            }
        };
        encode_buf.truncate(len);
        let encoded = encode_buf.as_slice();

        // Build the JSON event directly into `raw_event_buf` — avoids allocating
        // a full serde_json::Value tree (+ heap Map + String) on every flush.
        // Reused across flushes; cleared (capacity retained) here.
        //
        // Ownership of the String before `emit` is still required to avoid the
        // cross-task borrow race documented below, so we serialize into the
        // reused owned String rather than into a borrowed Value.
        raw_event_buf.clear();
        // {"sessionId":"<id>","data":"<b64>"}  → 16 + id + 9 + b64 + 2
        raw_event_buf.reserve(session_id.len() + encoded.len() + 32);
        raw_event_buf.push_str("{\"sessionId\":\"");
        raw_event_buf.push_str(session_id);
        raw_event_buf.push_str("\",\"data\":\"");
        // SAFETY-ish: base64 STANDARD output is ASCII-only, valid UTF-8.
        raw_event_buf.push_str(std::str::from_utf8(encoded).unwrap_or(""));
        raw_event_buf.push_str("\"}");
        if let Err(e) = app_handle.emit("pty:raw", raw_event_buf.as_str()) {
            log::warn!("Failed to emit pty:raw event: {}", e);
        }
        coalesce_buf.clear();
    }

    loop {
        let n: usize = tokio::select! {
            // `biased` ensures the read branch is preferred when data is
            // available, so we drain the PTY eagerly without dropping
            // completed reads because of an interval tick.
            biased;

            result = session.read_bytes(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        // Check the startup lease even while the shell is idle;
                        // otherwise a failed listener attach could remain
                        // muted forever without a successful PTY read.
                        let _ = session.expire_startup_pause();
                        // `Ok(0)` on a non-blocking fd means no data is
                        // available (EAGAIN) — a lull in output. Flush any
                        // pending coalesced data so the frontend gets prompt
                        // feedback after a burst of output.
                        if !coalesce_buf.is_empty()
                            && !session.raw_paused.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            flush_pty_raw(&mut coalesce_buf, &mut encode_buf, &mut raw_event_buf, &app_handle, &session_id);
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                        continue;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        let _ = session.expire_startup_pause();
                        // Flush any pending data before handling the error
                        // so the frontend doesn't lose the tail of output.
                        if !coalesce_buf.is_empty()
                            && !session.raw_paused.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            flush_pty_raw(&mut coalesce_buf, &mut encode_buf, &mut raw_event_buf, &app_handle, &session_id);
                        }
                        log::warn!("PTY read error for {}: {}", session_id, e);
                        if e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.kind() == std::io::ErrorKind::InvalidData
                        {
                            break;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        continue;
                    }
                }
            }

            _ = flush_interval.tick() => {
                // Timer-based flush: also owns the startup-pause safety check
                // so recovery does not depend on PTY output activity.
                let _ = session.expire_startup_pause();
                // Timer-based flush: guarantees the frontend receives data
                if !coalesce_buf.is_empty()
                    && !session.raw_paused.load(std::sync::atomic::Ordering::Relaxed)
                {
                    flush_pty_raw(&mut coalesce_buf, &mut encode_buf, &mut raw_event_buf, &app_handle, &session_id);
                }
                continue;
            }
        };

        log::trace!("pty_read_loop[{}]: read {} bytes", session_id, n);

        // Convert raw PTY bytes to text and append to output buffer
        let text = String::from_utf8_lossy(&read_buf[..n]);
        output_buffer.append_output(&session_id, &text, None);

        // Feed the agent-activity tracker: an output pulse marks an active
        // agent as working; OSC 633 CommandFinished is a supplementary
        // completion signal (the tracker ignores unrelated commands).
        if let Some(ref tracker) = agent_activity {
            tracker.on_pty_output_for_generation(&session_id, now_ms(), registration_generation);
            let sequences = shell_parser.feed(&text);
            for seq in &sequences {
                for ev in athena_core::shell_integration::process_sequences(
                    &mut shell_tracker,
                    std::slice::from_ref(seq),
                    &session_id,
                ) {
                    if let athena_core::shell_integration::ShellIntegrationEvent::CommandFinished {
                        command,
                        exit_code,
                        ..
                    } = ev
                    {
                        tracker.on_shell_command_finished_with_exit_code_for_generation(
                            &session_id,
                            &command,
                            exit_code,
                            now_ms(),
                            registration_generation,
                        );
                    }
                }
            }
        }

        // Step 1: parse the same bytes for the legacy cell-grid frontend.
        // `parse_bytes` returns `None` when no cells changed, in which
        // case we skip the structured event entirely.
        // For xterm.js sessions, we still parse (to keep VTE state fresh)
        // but skip emitting `terminal:data` — xterm.js parses raw ANSI itself.
        match session.parse_bytes_with_responses(&read_buf[..n]).await {
            Ok((Some(update), responses)) => {
                if !did_emit_ready {
                    did_emit_ready = true;
                    session.mark_ready().await;
                }
                for response in responses {
                    if let Err(error) = session.write(&response).await {
                        log::debug!(
                            "pty_read_loop[{}]: DSR response write failed: {}",
                            session_id,
                            error
                        );
                    }
                }
                // Skip cell-delta emission for xterm sessions — they have their
                // own ANSI parser and do not consume `terminal:data` events.
                if session.is_xterm.load(std::sync::atomic::Ordering::Relaxed) {
                    // xterm sessions handle readiness internally; skip data
                } else {
                    let event_data = serde_json::json!({
                        "sessionId": session_id,
                        "deltas": update.deltas,
                        "cursorRow": update.cursor_row,
                        "cursorCol": update.cursor_col,
                        "rows": update.rows,
                        "cols": update.cols,
                        "cursorVisible": update.cursor_visible,
                    });
                    let event_data_str = match serde_json::to_string(&event_data) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!(
                                "pty_read_loop[{}]: failed to serialize event_data: {}",
                                session_id,
                                e
                            );
                            continue;
                        }
                    };
                    if let Err(e) = app_handle.emit("terminal:data", event_data_str) {
                        log::warn!("Failed to emit terminal:data event: {}", e);
                    }
                }
            }
            Ok((None, responses)) => {
                if !did_emit_ready {
                    did_emit_ready = true;
                    session.mark_ready().await;
                }
                for response in responses {
                    if let Err(error) = session.write(&response).await {
                        log::debug!(
                            "pty_read_loop[{}]: DSR response write failed: {}",
                            session_id,
                            error
                        );
                    }
                }
            }
            Err(e) => {
                log::warn!("PTY parse error for {}: {}", session_id, e);
            }
        }

        // Step 2: accumulate raw bytes into the coalescing buffer.
        coalesce_buf.extend_from_slice(&read_buf[..n]);

        // Step 3: size-threshold flush — prevents unbounded growth when
        // commands like `yes` produce continuous output.
        // Emergency flush at the hard 1 MB cap, normal flush at 32 KB.
        // Skip ALL threshold flushes when raw_paused — the coalesce_buf
        // accumulates until the frontend unpauses after remount. 1 MB is
        // ~10k lines, far more than any swap window (typically <300ms).
        // If we flushed while paused, the bytes would hit a dead listener
        // and be lost — the exact desync we're fixing.
        let _ = session.expire_startup_pause();
        let now_paused = session
            .raw_paused
            .load(std::sync::atomic::Ordering::Relaxed);
        if now_paused {
            // Self-heal: detect the true → false transition on a *later*
            // iteration (the flag is cleared by the remount's
            // `pty_attach_listener` from
            // another task). On the first iteration where we observe it
            // cleared, flush everything accumulated while paused as a single
            // burst — exactly the behavior the remount expects. This decouples
            // the burst flush from *who* cleared the flag and makes a stuck-
            // paused-then-revived session recover automatically.
            if was_paused {
                // Still paused: enforce the backstop cap. All real thresholds
                // are skipped while paused, so without this `coalesce_buf`
                // grows unboundedly under a chatty shell during a stalled
                // remount. Drop the oldest half (preserving the most recent
                // output the remount will surface) instead of holding multi-MB
                // per pane. See `PAUSED_MAX_COALESCE_SIZE` rationale above.
                if coalesce_buf.len() >= PAUSED_MAX_COALESCE_SIZE {
                    let keep_from = coalesce_buf.len() / 2;
                    coalesce_buf.drain(..keep_from);
                    log::warn!(
                        "pty_read_loop[{}]: paused coalesce_buf hit {} KB backstop, dropped oldest half",
                        session_id,
                        PAUSED_MAX_COALESCE_SIZE / 1024
                    );
                }
            }
        } else {
            if was_paused {
                // true → false transition: flush the burst accumulated while
                // paused, writing cleanly on top of the remount's replayed
                // snapshot (or empty terminal). This runs regardless of how
                // the flag was cleared — frontend remount or backend attach.
                flush_pty_raw(
                    &mut coalesce_buf,
                    &mut encode_buf,
                    &mut raw_event_buf,
                    &app_handle,
                    &session_id,
                );
            }
            if coalesce_buf.len() >= 32 * 1024 {
                flush_pty_raw(
                    &mut coalesce_buf,
                    &mut encode_buf,
                    &mut raw_event_buf,
                    &app_handle,
                    &session_id,
                );
            }
        }
        was_paused = now_paused;

        // Rate limit: yield after each successful read to prevent CPU spin
        // when commands like `yes` produce infinite output.
        tokio::task::yield_now().await;
    }

    log::info!("PTY read loop exited for session: {}", session_id);
    // Keep the reader claim held on this terminal instance until all final
    // output and exit signaling have completed. SessionManager can then safely
    // replace the exited map entry on a deliberate respawn.

    // Flush any remaining coalesced data before signaling exit so the
    // frontend doesn't miss the tail of the session's output.
    if !coalesce_buf.is_empty() {
        flush_pty_raw(
            &mut coalesce_buf,
            &mut encode_buf,
            &mut raw_event_buf,
            &app_handle,
            &session_id,
        );
    }

    // The read loop is the authoritative PTY lifecycle boundary. Remove the
    // pane from the backend activity tracker before notifying the frontend so
    // a closed session cannot be resurrected by the heartbeat or retain stale
    // agent state indefinitely.
    if let Some(ref tracker) = agent_activity {
        tracker.remove_pane_if_generation(&session_id, registration_generation);
    }

    let exit_payload = serde_json::json!({
        "paneId": session_id,
        "generation": registration_generation,
    });
    if let Err(e) = app_handle.emit("terminal:exit", exit_payload.to_string()) {
        log::warn!("Failed to emit terminal:exit event: {}", e);
    }
    // This is the final lifecycle operation: a later spawn can now replace
    // the map entry without racing this reader's output or exit event.
    session.mark_exited().await;
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Write data to a PTY session's stdin.
#[tauri::command]
pub async fn pty_write(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    validate_session_id(&id)?;
    validate_data_size(data.as_bytes(), "pty_write data")?;
    // Grab the session Arc under a short lock, then release the global
    // session_manager mutex before the (potentially blocking) write. This
    // stops one big paste from serializing every other pane's keystrokes
    // behind it. `get_session` returns a cloned Arc and drops its read lock
    // before returning, so the write below is lock-free.
    let session = {
        let session_manager = state.session_manager.lock().await;
        session_manager
            .get_session(&id)
            .await
            .ok_or_else(|| "session not found".to_string())?
    };
    let written = session
        .write(data.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    if written != data.len() {
        return Err(format!(
            "pty_write partial write: {} of {} bytes (would-block retries exhausted on full PTY pipe)",
            written,
            data.len()
        ));
    }
    Ok(())
}

/// Read the OS clipboard as plain text.
#[tauri::command]
pub async fn read_clipboard_text(app_handle: tauri::AppHandle) -> Result<String, String> {
    app_handle
        .clipboard()
        .read_text()
        .map_err(|e| e.to_string())
}

/// Kill a PTY session by its ID.
#[tauri::command]
pub async fn pty_kill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // Natural process exits are handled by the activity heartbeat/read loop;
    // this explicit command is the user/tool cancellation path. Only emit a
    // cancellation after a real session was killed, avoiding false alerts for
    // stale pane ids or failed kill requests.
    let session_manager = state.session_manager.lock().await;
    let existed = session_manager.get_session(&id).await.is_some();
    let result = session_manager.kill(&id).await;
    drop(session_manager);
    if result.is_ok() && existed {
        state.agent_activity.cancel_pane(&id, now_ms());
    }
    result.map_err(|e| e.to_string())
}

/// Resize a PTY session's terminal dimensions.
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    log::info!(
        "pty_resize requested: id={} cols={} rows={}",
        id,
        cols,
        rows
    );
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.resize(&id, cols, rows).await;
    drop(session_manager);
    result.map_err(|e| e.to_string())
}

/// Get the accumulated output history for a PTY session.
/// Returns the current grid state as a JSON array of rows with cell characters.
#[tauri::command]
pub async fn pty_get_history(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        let grid = s.grid.lock().await;
        let mut rows_json = Vec::new();
        for row in &grid.rows {
            let chars: Vec<String> = row.iter().map(|c| c.c.to_string()).collect();
            rows_json.push(serde_json::json!({ "cells": chars }));
        }
        return serde_json::to_string(&serde_json::json!({
            "rows": rows_json,
            "cursor_row": grid.cursor.row,
            "cursor_col": grid.cursor.col,
        }))
        .map_err(|e| e.to_string());
    }
    Ok("null".to_string())
}

/// Check whether a PTY session with the given ID exists.
#[tauri::command]
pub async fn pty_has_session(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let session_manager = state.session_manager.lock().await;
    let result = session_manager.has_session(&id).await;
    drop(session_manager);
    Ok(result)
}

/// Check whether a PTY session's shell prompt is visible (ready).
/// Returns true only when the session status is Ready (shell has started).
#[tauri::command]
pub async fn pty_is_ready(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let session_manager = state.session_manager.lock().await;
    let result = match session_manager.get_session(&id).await {
        Some(session) => {
            let status = session.status.lock().await;
            *status == athena_terminal::session::PtyStatus::Ready
        }
        None => false,
    };
    drop(session_manager);
    Ok(result)
}

/// Get the working directory of a PTY session, if known.
#[tauri::command]
pub async fn pty_get_cwd(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        Ok(Some(s.cwd.clone()))
    } else {
        Ok(None)
    }
}

/// Structured info about a PTY session's current foreground process.
#[derive(Debug, Clone, serde::Serialize)]
struct AgentInfo {
    foreground_process: String,
    task_title: Option<String>,
    /// Session ID from the agent's history file, used to avoid
    /// re-summarizing the same session on every poll.
    session_id: Option<String>,
    /// Unix timestamp (ms) of the last prompt for the session.
    timestamp: Option<u64>,
    /// Raw prompt text (available for LLM summarization). Only set when
    /// the feature is enabled so the frontend can call the summarizer.
    raw_prompt: Option<String>,
}

// Agent history scraping and foreground classification now live in
// `athena_core::agent_detection` (shared, unit-tested, covers claude/codex/
// qwen/aider + the full known-agent roster).

/// Get the active foreground process and, if it's a known agent, try to
/// extract its current task title from the agent's own state files.
#[tauri::command]
pub async fn pty_agent_info(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    let Some(s) = session else {
        let info = AgentInfo {
            foreground_process: "shell".to_string(),
            task_title: None,
            session_id: None,
            timestamp: None,
            raw_prompt: None,
        };
        return serde_json::to_string(&info).map_err(|e| e.to_string());
    };

    // Use tcgetpgrp to get the ACTUAL foreground process group of the
    // controlling terminal, not the shell's stored pgid. When the user runs
    // `claude` interactively, zsh/bash job control puts it into a new
    // process group. tcgetpgrp(master_fd) returns that foreground group.
    let mut pgid = s.pgid.as_raw();
    let master_fd = s.master_fd.load(std::sync::atomic::Ordering::Acquire);
    if master_fd >= 0 {
        let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
        if fg_pgid > 0 {
            pgid = fg_pgid;
        }
    }
    if pgid <= 0 {
        let info = AgentInfo {
            foreground_process: "shell".to_string(),
            task_title: None,
            session_id: None,
            timestamp: None,
            raw_prompt: None,
        };
        return serde_json::to_string(&info).map_err(|e| e.to_string());
    }

    // Get the full command line for each process in the PTY's process group.
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ps")
            .args(["-o", "command=", "-g", &pgid.to_string()])
            .output()
    })
    .await
    .map_err(|e| e.to_string())?;

    let process = match output {
        Ok(out) if out.status.success() => athena_core::agent_detection::classify_foreground_ps(
            std::str::from_utf8(&out.stdout).unwrap_or(""),
        ),
        _ => "shell".to_string(),
    };

    // Try to extract the task title and session metadata from the agent's
    // own state files (delegates to agent_detection, which covers
    // claude/codex/qwen/aider).
    let (task_title, session_id, timestamp, raw_prompt) =
        match athena_core::agent_detection::scrape_agent_history(&process) {
            Some(h) => (
                Some(h.task_title),
                Some(h.session_id),
                Some(h.timestamp_ms),
                Some(h.raw_prompt),
            ),
            None => (None, None, None, None),
        };

    let info = AgentInfo {
        foreground_process: process,
        task_title,
        session_id,
        timestamp,
        raw_prompt,
    };
    serde_json::to_string(&info).map_err(|e| e.to_string())
}

/// Get the name of the active foreground process under a PTY session.
/// Uses `lsof` to find which command currently has the PTY's tty open.
/// Returns `None` if the session doesn't exist or the foreground cannot be determined.
#[tauri::command]
pub async fn pty_foreground_process(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let session_manager = state.session_manager.lock().await;
    let session = session_manager.get_session(&id).await;
    drop(session_manager);

    if let Some(s) = session {
        // Use tcgetpgrp to get the ACTUAL foreground process group of the
        // controlling terminal, not the shell's stored pgid.
        let mut pgid = s.pgid.as_raw();
        let master_fd = s.master_fd.load(std::sync::atomic::Ordering::Acquire);
        if master_fd >= 0 {
            let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
            if fg_pgid > 0 {
                pgid = fg_pgid;
            }
        }
        if pgid <= 0 {
            return Ok("shell".to_string());
        }

        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("ps")
                .args(["-o", "command=", "-g", &pgid.to_string()])
                .output()
        })
        .await
        .map_err(|e| e.to_string())?;

        match output {
            Ok(out) if out.status.success() => {
                Ok(athena_core::agent_detection::classify_foreground_ps(
                    std::str::from_utf8(&out.stdout).unwrap_or(""),
                ))
            }
            _ => Ok("shell".to_string()),
        }
    } else {
        Ok("shell".to_string())
    }
}

/// Agent CLI labels reported by [`athena_core::agent_detection`]. A session
/// whose foreground process classifies to one of these is treated as an agent
/// pane during app-exit resume capture. Re-exported from `athena-core` so
/// `commands/resume.rs` keeps working unchanged.
pub(crate) use athena_core::agent_detection::AGENT_FG_NAMES;

/// Best-effort classification of a single live session's controlling-terminal
/// foreground process (covers `claude` typed inside a plain shell pane).
/// Returns a label like `"claude"`/`"shell"`. Used by the app-exit capture to
/// decide which panes are agents worth nudging with `/exit`.
pub(crate) async fn session_foreground_label(
    session: &Arc<athena_terminal::session::TerminalSession>,
) -> String {
    let mut pgid = session.pgid.as_raw();
    let master_fd = session.master_fd.load(std::sync::atomic::Ordering::Acquire);
    if master_fd >= 0 {
        let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
        if fg_pgid > 0 {
            pgid = fg_pgid;
        }
    }
    if pgid <= 0 {
        return "shell".to_string();
    }
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ps")
            .args(["-o", "command=", "-g", &pgid.to_string()])
            .output()
    })
    .await;
    match output {
        Ok(Ok(out)) if out.status.success() => {
            athena_core::agent_detection::classify_foreground_ps(
                std::str::from_utf8(&out.stdout).unwrap_or(""),
            )
        }
        _ => "shell".to_string(),
    }
}

/// Spawn a new PTY session with the agent command to execute after startup.
/// The `agent_cmd` is executed in the shell after the PTY is set up.
#[tauri::command]
// Keep the PTY lifecycle handshake explicit at this Tauri wire boundary.
#[allow(clippy::too_many_arguments)]
pub async fn pty_spawn_agent(
    state: State<'_, AppState>,
    id: String,
    cwd: String,
    shell: String,
    agent_cmd: String,
    cols: Option<u16>,
    rows: Option<u16>,
    start_paused: Option<bool>,
    listener_owner: Option<String>,
) -> Result<(), String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);
    // Validate caller-supplied values (same gates as pty_spawn) plus bound the
    // agent command payload before it is written to the PTY.
    validate_session_id(&id)
        .inspect_err(|e| log::warn!("pty_spawn_agent rejected (bad id): {}", e))?;
    let validated_shell = validate_shell(&shell)
        .inspect_err(|_| log::warn!("pty_spawn_agent rejected: invalid shell input"))?;
    let validated_cwd = validate_cwd(&state.store, &cwd).map_err(|e| {
        log::warn!("pty_spawn_agent rejected: invalid workspace cwd");
        e.to_string()
    })?;
    validate_data_size(agent_cmd.as_bytes(), "agent_cmd")?;
    let shell_str = validated_shell.to_string_lossy().to_string();
    let cwd_str = validated_cwd.to_string_lossy().to_string();

    let integration_script = shell_integration_script(&shell_str);
    let session_manager = state.session_manager.lock().await;
    let session_result = session_manager
        .spawn_with_startup_script(
            id.clone(),
            &shell_str,
            &cwd_str,
            cols,
            rows,
            integration_script.as_deref(),
        )
        .await;
    drop(session_manager);

    match session_result {
        Ok(session) => {
            let _session_id = id.clone();
            let app_handle = state.app_handle.lock().clone();

            // `SessionManager::spawn` returns an existing Arc for duplicate
            // IDs. Only the caller that claims the session lifecycle may write
            // the startup command; otherwise a duplicate agent request would
            // execute the same command twice in the same shell.
            let claimed_reader = session.try_claim_read_loop();
            if claimed_reader {
                // Hold raw output until the xterm listener has subscribed.
                // Agent CLIs render immediately during startup; without this
                // handshake their first screen is lost before the frontend can
                // mount the terminal, producing a blank or half-painted pane.
                if start_paused.unwrap_or(false) {
                    session.begin_startup_pause(listener_owner.clone());
                }
                if let Err(e) = session.write(agent_cmd.as_bytes()).await {
                    session.release_read_loop();
                    log::error!("Failed to write agent command to PTY: {}", e);
                    return Err(e.to_string());
                }
            } else {
                log::debug!("pty_spawn_agent: session {} already initialized", id);
            }

            let agent_key = athena_core::agent_detection::AGENT_FG_NAMES
                .iter()
                .copied()
                .find(|key| athena_core::agent_detection::command_contains_agent(&agent_cmd, key))
                .unwrap_or("agent");
            state
                .agent_activity
                .notify_agent_started(&id, agent_key, now_ms());

            if let Some(handle) = app_handle {
                if claimed_reader {
                    let session_id_for_loop = id.clone();
                    let output_buffer = std::sync::Arc::clone(&state.output_buffer);
                    let tracker = std::sync::Arc::clone(&state.agent_activity);
                    tokio::spawn(async move {
                        pty_read_loop(
                            handle,
                            session_id_for_loop,
                            session,
                            output_buffer,
                            Some(tracker),
                        )
                        .await;
                    });
                }
            } else if claimed_reader {
                // There is no reader to release this claim. A later spawn
                // with an app handle must be allowed to start the loop.
                session.release_read_loop();
            }

            log::info!("PTY agent session spawned: id={}", id);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to spawn PTY agent session: {}", e);
            Err(e.to_string())
        }
    }
}

/// Mark a PTY session as being rendered by xterm.js.
///
/// When a session is xterm-backed, the backend skips emitting the
/// `terminal:data` cell-delta events because xterm.js parses raw ANSI
/// bytes itself.  This eliminates wasted VTE work, JSON serialization,
/// and IPC for those sessions.
#[tauri::command]
pub async fn pty_set_xterm(
    state: State<'_, AppState>,
    id: String,
    is_xterm: bool,
) -> Result<(), String> {
    let sm = state.session_manager.lock().await;
    if let Some(session) = sm.get_session(&id).await {
        session
            .is_xterm
            .store(is_xterm, std::sync::atomic::Ordering::Relaxed);
        log::debug!("pty_set_xterm: {} -> {}", id, is_xterm);
        Ok(())
    } else {
        Err(format!("Session {} not found", id))
    }
}

/// Signal that a frontend listener has (re)subscribed to `pty:raw` for `id`.
///
/// This is the explicit "someone is listening again" handshake, called by the
/// xterm.js mount right after `pty_listen_raw` subscribes. It clears
/// `raw_paused` so the read loop's next iteration observes the true → false
/// transition and flushes the accumulated burst (see `pty_read_loop`). The
/// flush itself runs in the read loop, not here — `coalesce_buf` is owned by
/// the read task, so the flag is the only IPC needed.
///
#[tauri::command]
pub async fn pty_attach_listener(
    state: State<'_, AppState>,
    id: String,
    owner: String,
    replace_current: Option<bool>,
) -> Result<String, String> {
    validate_session_id(&id)?;
    let sm = state.session_manager.lock().await;
    if let Some(session) = sm.get_session(&id).await {
        let generation = session
            .attach_listener(owner, replace_current.unwrap_or(false))
            .ok_or_else(|| "a newer listener is already attached".to_string())?;
        log::debug!(
            "pty_attach_listener: {} unpaused (listener generation {})",
            id,
            generation
        );
        Ok(generation.to_string())
    } else {
        // Session may not exist yet on a brand-new spawn. Return generation 0;
        // the frontend retries after the spawn/read-loop boundary.
        Ok("0".to_string())
    }
}

/// Detach one frontend raw-output listener generation.
///
/// Only the currently active generation may pause raw output. This makes
/// teardown ordering safe when a pane remounts while the old attach/detach IPC
/// calls are still in flight.
#[tauri::command]
pub async fn pty_detach_listener(
    state: State<'_, AppState>,
    id: String,
    owner: String,
    generation: String,
) -> Result<bool, String> {
    validate_session_id(&id)?;
    let generation = generation
        .parse::<u64>()
        .map_err(|_| "invalid listener generation".to_string())?;
    let sm = state.session_manager.lock().await;
    if let Some(session) = sm.get_session(&id).await {
        let paused = if generation == 0 {
            session.cancel_startup_pause(&owner)
        } else {
            session.detach_listener(&owner, generation)
        };
        log::debug!(
            "pty_detach_listener: {} generation {} {}",
            id,
            generation,
            if paused { "paused" } else { "stale" }
        );
        Ok(paused)
    } else {
        Ok(false)
    }
}
