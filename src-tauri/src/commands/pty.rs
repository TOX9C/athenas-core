//! PTY command handlers.

use super::{validate_path_exists, CommandError};
use crate::state::AppState;
use base64::Engine;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

// ── PTY spawn/write validation helpers ───────────────────────────────────────
//
// The IPC boundary is the entire frontend renderer. A compromised or XSSed
// renderer (CSP permits `wasm-unsafe-eval` + inline styles — required by the
// Dioxus WASM frontend, see tauri.conf.json `security.csp`) could otherwise
// spawn an arbitrary binary rooted at `~/.ssh` or `/`, or paste unbounded
// data into a live PTY. These helpers gate the shell binary, the working
// directory, and payload sizes.
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

/// Background task that reads PTY output and fans it out to consumers.
///
/// Outputs, in coalesced batches (one delivery per 8 ms flush tick):
/// - the session's raw sink — a `Channel<Vec<u8>>` installed by the xterm.js
///   mount's attach handshake, carrying raw PTY bytes as binary IPC (no
///   base64, no per-flush webview eval);
/// - `pty:raw:<id>` — the legacy base64-encoded event stream, emitted only
///   while a Mobile Mirror relay phone is subscribed
///   (`AppState::relay_raw_subscribers`);
/// - `terminal:data` — parsed cell deltas for the legacy cell-grid frontend,
///   emitted only when the grid actually changed.
///
/// Reads come from the session's dedicated blocking reader thread
/// (`TerminalSession::spawn_reader`); raw bytes are also appended to a
/// bounded per-pane replay buffer for relay reconnects.
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
    // Agent lifecycle push protocol (OSC 6337): agents/plugins report their
    // own complete / needs-input / error state in-band, and we relay it to
    // the activity tracker immediately (no heartbeat poll).
    let mut lifecycle_parser = athena_core::agent_lifecycle::AgentLifecycleParser::new();

    // UTF-8 carry buffer. A multi-byte character can be split across two
    // reads; `from_utf8_lossy` on each chunk independently would corrupt it
    // into U+FFFD. We hold the incomplete tail here until the next read
    // completes the sequence.
    let mut utf8_carry: Vec<u8> = Vec::with_capacity(4);

    // Coalescing buffer for `pty:raw` PTY output. Pre-allocate to 32 KB
    // to avoid reallocation churn during active output.
    let mut coalesce_buf: Vec<u8> = Vec::with_capacity(32 * 1024);
    // Escape the session ID once; flushes reuse this JSON fragment on the hot
    // path instead of allocating a new escaped string every 8 ms.
    let escaped_session_id =
        serde_json::to_string(&session_id).unwrap_or_else(|_| "\"\"".to_string());
    // Reusable base64 output buffer. `flush_pty_raw` can fire up to 125×/sec
    // per session; without reuse each flush allocates a fresh base64 String.
    // We instead encode into this buffer with `encode_slice` (zero-alloc) and
    // grow it lazily — capacity is a monotonic high-water mark, so steady-state
    // flushes hit zero reallocs.
    let mut encode_buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    // Reusable JSON event string. Same rationale — avoids a per-flush String
    // + serde_json::Value tree allocation. Cleared (not freed) between flushes.
    let mut raw_event_buf: String = String::with_capacity(64 * 1024);
    // perf#6: legacy cell-grid coalescing. The non-xterm path used to emit a
    // `terminal:data` event per read (full delta JSON per 16 KB chunk — the
    // same per-chunk fan-out perf#1 removed from the raw path). Deltas merge
    // last-wins per (row, col) cell; cursor/size metadata keeps the latest
    // parse. The 8 ms flush tick below emits one merged event.
    let mut grid_deltas: std::collections::BTreeMap<
        (usize, usize),
        athena_terminal::grid::CellDelta,
    > = std::collections::BTreeMap::new();
    let mut grid_meta: Option<(usize, usize, usize, usize, bool)> = None;
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

    /// Flush accumulated raw PTY bytes.
    ///
    /// Primary delivery is the attached xterm listener's raw IPC channel
    /// (installed by `pty_attach_listener`): bytes cross as-is — no base64,
    /// no JSON, and no webview JS eval per flush. The base64 `pty:raw:<id>`
    /// event path remains for the Mobile Mirror relay and only runs while a
    /// phone actually subscribes; `Emitter::emit` evals webview JS even with
    /// zero listeners, so unconditional emission wasted work on every flush.
    ///
    /// `encode_buf` and `raw_event_buf` are reusable scratch buffers so the
    /// relay path allocates zero after warmup.
    fn flush_pty_raw(
        coalesce_buf: &mut Vec<u8>,
        encode_buf: &mut Vec<u8>,
        raw_event_buf: &mut String,
        app_handle: &tauri::AppHandle,
        session_id: &str,
        escaped_session_id: &str,
    ) {
        if coalesce_buf.is_empty() {
            return;
        }

        // Keep a bounded raw replay buffer for the Mobile Mirror relay: a
        // reconnecting phone replays these exact bytes to restore VT screen
        // state that ANSI-stripped text history cannot. Unconditional — a
        // phone can subscribe mid-stream. Running-total store: O(1) here.
        let state = app_handle.try_state::<crate::state::AppState>();
        if let Some(state) = &state {
            state
                .relay_raw_replay
                .lock()
                .append(session_id, coalesce_buf, now_ms());
        }

        {
            // Relay phones consume base64 `pty:raw:<id>` events. base64
            // expands input by ~4/3; `encode_slice` into the reused buffer
            // keeps this zero-alloc after warmup.
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

            raw_event_buf.clear();
            raw_event_buf.reserve(escaped_session_id.len() + len + 27);
            raw_event_buf.push_str("{\"sessionId\":");
            raw_event_buf.push_str(escaped_session_id);
            raw_event_buf.push_str(",\"data\":\"");
            // SAFETY-ish: base64 STANDARD output is ASCII-only, valid UTF-8.
            raw_event_buf.push_str(std::str::from_utf8(&encode_buf[..len]).unwrap_or(""));
            raw_event_buf.push_str("\"}");
            let raw_channel = format!("pty:raw:{session_id}");
            if let Err(e) = app_handle.emit(&raw_channel, raw_event_buf.as_str()) {
                log::warn!("Failed to emit pty:raw event: {}", e);
            }
        }
        coalesce_buf.clear();
    }

    /// Flush merged legacy cell-grid deltas as one `terminal:data` event.
    ///
    /// perf#6: the non-xterm path previously emitted per read; this coalesces
    /// to one event per 8 ms tick (last write per cell wins, latest cursor
    /// metadata). No-op when nothing is pending.
    fn flush_grid_deltas(
        grid_deltas: &mut std::collections::BTreeMap<
            (usize, usize),
            athena_terminal::grid::CellDelta,
        >,
        grid_meta: &mut Option<(usize, usize, usize, usize, bool)>,
        app_handle: &tauri::AppHandle,
        session_id: &str,
    ) {
        if grid_deltas.is_empty() {
            return;
        }
        let taken = std::mem::take(grid_deltas);
        let deltas: Vec<athena_terminal::grid::CellDelta> = taken.into_values().collect();
        let (cursor_row, cursor_col, rows, cols, cursor_visible) = grid_meta.unwrap_or_default();
        let event_data = serde_json::json!({
            "sessionId": session_id,
            "deltas": deltas,
            "cursorRow": cursor_row,
            "cursorCol": cursor_col,
            "rows": rows,
            "cols": cols,
            "cursorVisible": cursor_visible,
        });
        match serde_json::to_string(&event_data) {
            Ok(event_data_str) => {
                if let Err(e) = app_handle.emit("terminal:data", event_data_str) {
                    log::warn!("Failed to emit terminal:data event: {}", e);
                }
            }
            Err(e) => {
                log::error!(
                    "pty_read_loop[{}]: failed to serialize grid deltas: {}",
                    session_id,
                    e
                );
            }
        }
    }

    // Dedicated blocking reader thread per session (`session.spawn_reader`):
    // delivers raw chunks over a channel. An idle pane costs exactly one
    // parked `poll(2)` — no per-wake fd dup / spawned poll / adaptive
    // backoff. Why not `tokio::io::AsyncFd`: the master fd is shared via an
    // atomic sentinel (`master_fd`) precisely so `close_fd` can fire from any
    // task (kill, duplicate_session, shutdown_all, Drop) and readers observe
    // the swap; AsyncFd would take ownership of the fd and force a
    // restructure of every close/respawn path.
    let mut read_rx = session.spawn_reader();

    loop {
        let data: Vec<u8> = tokio::select! {
            // `biased` ensures the read branch is preferred when data is
            // available, so we drain the PTY eagerly without dropping
            // completed reads because of an interval tick.
            biased;

            msg = read_rx.recv() => {
                match msg {
                    Some(Ok(bytes)) => bytes,
                    Some(Err(e)) => {
                        let _ = session.expire_startup_pause();
                        // Flush any pending data before handling the error
                        // so the frontend doesn't lose the tail of output.
                        if !coalesce_buf.is_empty()
                            && !session.raw_paused.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            flush_pty_raw(
                                &mut coalesce_buf,
                                &mut encode_buf,
                                &mut raw_event_buf,
                                &app_handle,
                                &session_id,
                                &escaped_session_id,
                            );
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
                    // Reader thread exited: fd closed (kill/shutdown) or the
                    // slave end hit EOF — the session is over.
                    None => break,
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
                    flush_pty_raw(
                                &mut coalesce_buf,
                                &mut encode_buf,
                                &mut raw_event_buf,
                                &app_handle,
                                &session_id,
                                &escaped_session_id,
                            );
                }
                // perf#6: flush merged legacy cell deltas on the same tick.
                if !grid_deltas.is_empty()
                    && !session.raw_paused.load(std::sync::atomic::Ordering::Relaxed)
                {
                    flush_grid_deltas(
                        &mut grid_deltas,
                        &mut grid_meta,
                        &app_handle,
                        &session_id,
                    );
                }
                continue;
            }
        };

        let n = data.len();
        log::trace!("pty_read_loop[{}]: read {} bytes", session_id, n);
        // Convert raw PTY bytes to text and append to the output buffer.
        // A multi-byte character can split across two reads: the carry
        // buffer holds the incomplete trailing sequence until its
        // continuation bytes arrive (F2 contract).
        let text = decode_pty_chunk(&mut utf8_carry, &data);
        output_buffer.append_output(&session_id, &text, None);

        // Feed the agent-activity tracker: an output pulse marks an active
        // agent as working; OSC 633 CommandFinished is a supplementary
        // completion signal (the tracker ignores unrelated commands).
        if let Some(ref tracker) = agent_activity {
            tracker.on_pty_output_for_generation(&session_id, now_ms(), registration_generation);
            for event in lifecycle_parser.feed(&text) {
                tracker.on_agent_lifecycle(&session_id, &event, registration_generation, now_ms());
            }
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

        // xterm.js is the authoritative ANSI/VTE parser for desktop and
        // mobile mounts. Do not parse the same bytes through the backend VTE
        // grid in that mode; this removes the largest redundant CPU/allocation
        // cost on the raw-output hot path. The legacy cell-grid path retains
        // the parser and its device-response handling.
        if session.is_xterm.load(std::sync::atomic::Ordering::Relaxed) {
            if !did_emit_ready {
                did_emit_ready = true;
                session.mark_ready().await;
            }
        } else {
            match session.parse_bytes_with_responses(&data).await {
                Ok((Some(update), responses)) => {
                    if !did_emit_ready {
                        did_emit_ready = true;
                        session.mark_ready().await;
                    }
                    // DSR/protocol replies stay latency-sensitive: write
                    // them immediately, do not wait for the 8 ms tick.
                    for response in responses {
                        if let Err(error) = session.write(&response).await {
                            log::debug!(
                                "pty_read_loop[{}]: DSR response write failed: {}",
                                session_id,
                                error
                            );
                        }
                    }
                    // perf#6: merge into the pending set (last write per
                    // cell wins) instead of emitting per read.
                    for d in update.deltas {
                        grid_deltas.insert((d.row, d.col), d);
                    }
                    grid_meta = Some((
                        update.cursor_row,
                        update.cursor_col,
                        update.rows,
                        update.cols,
                        update.cursor_visible,
                    ));
                    // Hard cap mirrors the raw path's 32 KB threshold: a
                    // full-screen scroll rewrites every cell, which can
                    // exceed 32k pending cells in one parse. Skipped while
                    // raw_paused — a flush would hit a dead listener; the
                    // pending set rides the unpause burst instead.
                    if grid_deltas.len() >= 32 * 1024
                        && !session
                            .raw_paused
                            .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        flush_grid_deltas(
                            &mut grid_deltas,
                            &mut grid_meta,
                            &app_handle,
                            &session_id,
                        );
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
        }

        // Step 2: accumulate raw bytes into the coalescing buffer.
        coalesce_buf.extend_from_slice(&data);

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
                    &escaped_session_id,
                );
                // perf#6: the unpause burst carries pending cell deltas so
                // the legacy grid replays in the same burst as raw bytes.
                flush_grid_deltas(&mut grid_deltas, &mut grid_meta, &app_handle, &session_id);
            }
            if coalesce_buf.len() >= 32 * 1024 {
                flush_pty_raw(
                    &mut coalesce_buf,
                    &mut encode_buf,
                    &mut raw_event_buf,
                    &app_handle,
                    &session_id,
                    &escaped_session_id,
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
            &escaped_session_id,
        );
    }
    // perf#6: drain pending legacy cell deltas before the exit signal so the
    // final frame lands before terminal:exit.
    flush_grid_deltas(&mut grid_deltas, &mut grid_meta, &app_handle, &session_id);

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

/// One pane's raw replay bytes plus the wall-clock time of its last write,
/// so the aggregate replay map can evict least-recently-written panes.
#[derive(Debug, Default)]
pub(crate) struct RelayReplayBuffer {
    pub data: Vec<u8>,
    pub last_write_ms: u64,
}

/// Bounded raw-replay store for the Mobile Mirror relay. Tracks a running
/// `total_bytes` so appends are O(1) — the previous per-flush
/// `values().map(len).sum()` was O(#panes) on the 8 ms hot path of every pane.
#[derive(Debug, Default)]
pub(crate) struct RelayReplayStore {
    map: std::collections::HashMap<String, RelayReplayBuffer>,
    total_bytes: usize,
}

impl RelayReplayStore {
    /// Cap for a single pane's replay buffer.
    const PER_PANE_MAX_BYTES: usize = 64 * 1024; // 64 KB
    /// Aggregate cap across all panes.
    const TOTAL_MAX_BYTES: usize = 4 * 1024 * 1024; // 4 MB

    /// Append raw bytes for a pane, enforcing the per-pane and aggregate caps.
    pub(crate) fn append(&mut self, pane_id: &str, bytes: &[u8], now: u64) {
        let entry = self
            .map
            .entry(pane_id.to_string())
            .or_insert_with(|| RelayReplayBuffer {
                data: Vec::new(),
                last_write_ms: now,
            });
        let before_len = entry.data.len();
        entry.data.extend_from_slice(bytes);
        entry.last_write_ms = now;
        let overflow = entry.data.len().saturating_sub(Self::PER_PANE_MAX_BYTES);
        if overflow > 0 {
            entry.data.drain(0..overflow);
        }
        self.total_bytes += entry.data.len() - before_len;
        self.enforce_total_cap();
    }

    /// Evict least-recently-written panes until the aggregate memory fits, so
    /// a long session with many never-killed panes cannot grow the map
    /// without bound.
    fn enforce_total_cap(&mut self) {
        if self.total_bytes <= Self::TOTAL_MAX_BYTES {
            return;
        }
        let mut stale: Vec<(String, u64)> = self
            .map
            .iter()
            .map(|(id, b)| (id.clone(), b.last_write_ms))
            .collect();
        stale.sort_by_key(|(_, ts)| *ts);
        for (id, _) in stale {
            if self.total_bytes <= Self::TOTAL_MAX_BYTES {
                break;
            }
            if let Some(buf) = self.map.remove(&id) {
                self.total_bytes -= buf.data.len();
            }
        }
    }

    /// Drop a pane's replay bytes, keeping the running total exact.
    pub(crate) fn remove(&mut self, pane_id: &str) -> Option<RelayReplayBuffer> {
        let removed = self.map.remove(pane_id);
        if let Some(buf) = &removed {
            self.total_bytes -= buf.data.len();
        }
        removed
    }

    /// Borrow a pane's replay bytes for a relay replay request.
    pub(crate) fn get(&self, pane_id: &str) -> Option<&[u8]> {
        self.map.get(pane_id).map(|b| b.data.as_slice())
    }
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
    // stale pane ids or failed kill requests. A single lookup both detects
    // existence and captures the pgid before the kill removes the session:
    // the shared foreground cache must not serve the dead session's label to
    // a recycled pgid for up to one TTL window.
    let session_manager = state.session_manager.lock().await;
    let killed_pgid = session_manager
        .get_session(&id)
        .await
        .map(|s| s.pgid.as_raw());
    let existed = killed_pgid.is_some();
    let result = session_manager.kill(&id).await;
    drop(session_manager);
    if let Some(pgid) = killed_pgid {
        athena_core::agent_detection::invalidate_foreground_cache(pgid);
    }
    if result.is_ok() && existed {
        state.agent_activity.forget_pane(&id);
    }
    // Drop the relay raw-replay buffer for the killed pane: its content is no
    // longer live and must not be replayed to a future pane reusing the id.
    state.relay_raw_replay.lock().remove(&id);
    result.map_err(|e| e.to_string())
}

/// Return the last ~64 KB of raw PTY bytes for a pane, base64-encoded, for
/// the Mobile Mirror relay to replay exact VT screen state after a reconnect.
/// Returns an empty string when the pane has no replay buffer (fresh spawn,
/// killed pane, or nothing flushed yet) — the mobile client then falls back
/// to ANSI-stripped text history.
#[tauri::command]
pub async fn pty_raw_replay(state: State<'_, AppState>, pane_id: String) -> Result<String, String> {
    let bytes = state
        .relay_raw_replay
        .lock()
        .get(&pane_id)
        .map(|slice| slice.to_vec())
        .unwrap_or_default();
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Resize a PTY session's terminal dimensions.
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    owner: Option<String>,
) -> Result<(), String> {
    log::info!(
        "pty_resize requested: id={} cols={} rows={} owner={:?}",
        id,
        cols,
        rows,
        owner
    );
    let session_manager = state.session_manager.lock().await;
    let result = session_manager
        .resize(&id, cols, rows, owner.as_deref())
        .await;
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

    // Resolve the live foreground process group (tcgetpgrp on the master fd,
    // shell pgid as fallback) and classify it through the shared TTL cache so
    // concurrent frontend bursts share one `ps` spawn per pgid.
    let master_fd = s.master_fd.load(std::sync::atomic::Ordering::Acquire);
    let process = tokio::task::spawn_blocking(move || {
        athena_core::agent_detection::resolve_foreground_label(master_fd, s.pgid.as_raw())
            .unwrap_or_else(|| "shell".to_string())
    })
    .await
    .map_err(|e| e.to_string())?;

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
        // Shared TTL-cached tcgetpgrp + ps classification (see
        // `resolve_foreground_label`): concurrent frontend bursts share one
        // `ps` spawn per pgid.
        let master_fd = s.master_fd.load(std::sync::atomic::Ordering::Acquire);
        let pgid = s.pgid.as_raw();
        tokio::task::spawn_blocking(move || {
            Ok(
                athena_core::agent_detection::resolve_foreground_label(master_fd, pgid)
                    .unwrap_or_else(|| "shell".to_string()),
            )
        })
        .await
        .map_err(|e| e.to_string())?
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
    let master_fd = session.master_fd.load(std::sync::atomic::Ordering::Acquire);
    let pgid = session.pgid.as_raw();
    tokio::task::spawn_blocking(move || {
        athena_core::agent_detection::resolve_foreground_label(master_fd, pgid)
            .unwrap_or_else(|| "shell".to_string())
    })
    .await
    .unwrap_or_else(|_| "shell".to_string())
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

/// Attach a frontend raw-output listener for `id` and resume output.
///
/// Attach registers a listener generation and clears `raw_paused`, so the
/// read loop's next iteration flushes the accumulated burst. Desktop and
/// relay (phone) callers share this path; both consume the base64
/// `pty:raw:<id>` event stream.
pub(crate) async fn pty_attach_listener_impl(
    state: &State<'_, AppState>,
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

/// Desktop attach from the xterm.js mount.
#[tauri::command]
pub async fn pty_attach_listener(
    state: State<'_, AppState>,
    id: String,
    owner: String,
    replace_current: Option<bool>,
) -> Result<String, String> {
    pty_attach_listener_impl(&state, id, owner, replace_current).await
}

/// Relay attach: identical handshake, callable from the relay dispatch path.
pub(crate) async fn pty_attach_listener_relay(
    state: State<'_, AppState>,
    id: String,
    owner: String,
    replace_current: Option<bool>,
) -> Result<String, String> {
    pty_attach_listener_impl(&state, id, owner, replace_current).await
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

/// Incrementally decode one PTY read chunk into UTF-8 text.
///
/// A multi-byte character can split across two reads: prepend any carried
/// bytes, then decode the longest valid UTF-8 runs. An incomplete trailing
/// sequence (`error_len() == None`, ≤3 bytes) stays in `carry` for the next
/// chunk; genuinely invalid bytes become U+FFFD, matching the previous
/// lossy behavior while preserving byte alignment.
pub(crate) fn decode_pty_chunk(carry: &mut Vec<u8>, chunk: &[u8]) -> String {
    let mut bytes = std::mem::take(carry);
    bytes.extend_from_slice(chunk);
    let mut text = String::new();
    loop {
        match std::str::from_utf8(&bytes) {
            Ok(valid) => {
                text.push_str(valid);
                break;
            }
            Err(e) => {
                let valid_end = e.valid_up_to();
                if valid_end > 0 {
                    // Proven-valid prefix; this unwrap cannot fire.
                    text.push_str(std::str::from_utf8(&bytes[..valid_end]).unwrap());
                }
                match e.error_len() {
                    None => {
                        // Incomplete sequence at the buffer end: carry
                        // it into the next read.
                        *carry = bytes.split_off(valid_end);
                        break;
                    }
                    Some(skip) => {
                        // Invalid bytes: one replacement char, resume
                        // decoding after them.
                        text.push('\u{FFFD}');
                        bytes.drain(..valid_end + skip);
                    }
                }
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{decode_pty_chunk, RelayReplayStore};

    #[test]
    fn replay_store_tracks_running_total_across_appends() {
        let mut store = RelayReplayStore::default();
        store.append("a", b"hello", 1);
        store.append("b", b"world!!", 2);
        store.append("a", b"[more]", 3);
        assert_eq!(store.total_bytes, 5 + 7 + 6);
        assert_eq!(store.get("a"), Some(b"hello[more]".as_slice()));
        assert_eq!(store.get("b"), Some(b"world!!".as_slice()));
    }

    #[test]
    fn replay_store_per_pane_cap_drops_oldest_and_keeps_total_exact() {
        let mut store = RelayReplayStore::default();
        let chunk = vec![b'x'; RelayReplayStore::PER_PANE_MAX_BYTES];
        store.append("a", &chunk, 1);
        store.append("a", &chunk, 2);
        let stored = store.get("a").expect("pane a must still exist");
        assert_eq!(stored.len(), RelayReplayStore::PER_PANE_MAX_BYTES);
        assert!(
            stored.iter().all(|b| *b == b'x'),
            "contents homogeneous after trim"
        );
        assert_eq!(store.total_bytes, RelayReplayStore::PER_PANE_MAX_BYTES);
    }

    #[test]
    fn replay_store_remove_updates_total() {
        let mut store = RelayReplayStore::default();
        store.append("a", b"12345", 1);
        store.append("b", b"123456", 1);
        assert_eq!(store.remove("a").unwrap().data.len(), 5);
        assert_eq!(store.total_bytes, 6);
        assert!(store.remove("a").is_none());
        assert_eq!(store.total_bytes, 6);
    }

    #[test]
    fn replay_store_aggregate_cap_evicts_least_recently_written() {
        let mut store = RelayReplayStore::default();
        // Fill at the per-pane cap: 4 MB aggregate cap holds 64 panes, so
        // pane 70 must force LRU eviction without letting the total exceed
        // the cap.
        let chunk = vec![b'y'; RelayReplayStore::PER_PANE_MAX_BYTES];
        let panes = RelayReplayStore::TOTAL_MAX_BYTES / RelayReplayStore::PER_PANE_MAX_BYTES + 6;
        for i in 0..panes {
            store.append(&format!("p{i}"), &chunk, i as u64);
        }
        assert!(store.get("p0").is_none(), "LRU pane must be evicted");
        assert!(store.get(&format!("p{}", panes - 1)).is_some());
        assert!(
            store.total_bytes <= RelayReplayStore::TOTAL_MAX_BYTES,
            "total {} must respect the cap {}",
            store.total_bytes,
            RelayReplayStore::TOTAL_MAX_BYTES
        );
        let retained: usize = (0..panes)
            .map(|i| store.get(&format!("p{i}")).map_or(0, |s| s.len()))
            .sum();
        assert_eq!(
            store.total_bytes, retained,
            "running total must match the retained pane bytes"
        );
    }

    #[test]
    fn f2_emoji_split_across_chunks_survives() {
        // "👍" = F0 9F 91 8D. Split the 4-byte sequence across two chunks;
        // neither chunk may emit U+FFFD, and the joined text must round-trip.
        let emoji = "👍";
        let bytes = emoji.as_bytes();
        assert_eq!(bytes.len(), 4);
        let mut carry = Vec::new();
        let first = decode_pty_chunk(&mut carry, &bytes[..3]);
        assert!(
            !first.contains('\u{FFFD}'),
            "split prefix must not corrupt: {first:?}"
        );
        assert!(first.is_empty(), "incomplete sequence yields no text yet");
        let second = decode_pty_chunk(&mut carry, &bytes[3..]);
        assert_eq!(format!("{first}{second}"), emoji);
    }

    #[test]
    fn f2_cjk_paste_split_across_chunks_survives() {
        // Two 3-byte CJK chars with the split landing mid-second-char.
        let text = "你好";
        let bytes = text.as_bytes();
        let mut carry = Vec::new();
        let mut out = decode_pty_chunk(&mut carry, &bytes[..4]);
        out.push_str(&decode_pty_chunk(&mut carry, &bytes[4..]));
        assert_eq!(out, text);
    }

    #[test]
    fn f2_genuinely_invalid_bytes_still_become_replacement() {
        let mut carry = Vec::new();
        let out = decode_pty_chunk(&mut carry, &[b'a', 0xFF, b'b']);
        assert_eq!(out, "a\u{FFFD}b");
        assert!(carry.is_empty());
    }

    #[test]
    fn f2_carry_capped_at_three_bytes_for_incomplete_prefix() {
        // A 4-byte-char prefix of 3 bytes must carry exactly 3 bytes.
        let bytes = "👍".as_bytes();
        let mut carry = Vec::new();
        let _ = decode_pty_chunk(&mut carry, &bytes[..1]);
        let _ = decode_pty_chunk(&mut carry, &bytes[1..2]);
        let _ = decode_pty_chunk(&mut carry, &bytes[2..3]);
        assert_eq!(carry.len(), 3);
        assert_eq!(carry, &bytes[..3]);
    }
}
