//! Agent-comms TCP connection lifecycle and message dispatch helpers.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};

use super::{
    generate_uuid, now_ms, validate_agent_message, AgentMessage, AgentSession, PendingInput,
    SessionInternal, SessionStatus, INPUT_REQUEST_TIMEOUT, MAX_AGENT_ID_BYTES,
    MAX_AGENT_LINE_BYTES,
};
use crate::EventEmitter;

fn send_to_socket(stream: &TcpStream, payload: &serde_json::Value) {
    if let Ok(mut w) = stream.try_clone() {
        let mut buf = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
        buf.push('\n');
        let _ = w.write_all(buf.as_bytes());
    }
}

fn emit_to_renderer(event_emitter: &EventEmitter, channel: &str, data: &serde_json::Value) {
    if let Ok(guard) = event_emitter.lock() {
        if let Some(ref emitter) = *guard {
            emitter(channel, data);
            return;
        }
    }
    // Agent messages may contain credentials, prompts, or workspace output.
    log::debug!("[agent-comms] event emitted on channel {channel}");
}

/// Drain the per-connection outbound queue onto the socket.
///
/// This is the only consumer of the channel created in [`handle_connection`];
/// `send_to_agent` / `broadcast_to_agents` only ever `try_send` into the
/// queue, so a slow or hung client never stalls the rest of the comms
/// service. `set_write_timeout` bounds each `write_all`; a failed write
/// exits the task. The task also exits when every sender is dropped
/// (session evicted / connection closed) or within [`WRITER_IDLE_POLL`]
/// of `alive` being cleared, whichever comes first.
fn writer_task(
    stream: TcpStream,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    alive: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut stream = stream;
    let _ = stream.set_write_timeout(Some(WRITER_WRITE_TIMEOUT));
    loop {
        match rx.recv_timeout(WRITER_IDLE_POLL) {
            Ok(mut bytes) => {
                bytes.push(b'\n');
                if let Err(e) = stream.write_all(&bytes) {
                    log::warn!("agent-comms: outbound write failed, closing writer: {}", e);
                    alive.store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !alive.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// How long a socket write from the writer task may block before the write
/// fails and the writer task exits. Only the dedicated writer task can ever
/// be stuck on `write_all`; the connection reader and the sessions mutex are
/// never affected.
const WRITER_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the writer task re-checks for disconnection while the outbound
/// queue is idle (i.e. how long the task may linger after its connection is
/// already closed, when no messages keep flowing).
const WRITER_IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// Outbound-channel capacity per connected agent. Larger than any realistic
/// burst of backend→agent messages; when full, `send_to_agent` and
/// `broadcast_to_agents` drop and log rather than stall the whole comms
/// service behind one slow agent.
const OUTBOUND_QUEUE_CAP: usize = 1024;

pub(super) fn handle_connection(
    stream: TcpStream,
    sessions: Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: Arc<Mutex<HashMap<String, PendingInput>>>,
    token: String,
    event_emitter: EventEmitter,
) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    log::info!("Agent comms: new connection from {}", peer);

    // Outbound queue consumed by a dedicated writer task so that
    // `send_to_agent` / `broadcast_to_agents` are pure in-memory
    // `try_send`s, never touching the socket — and never blocking or
    // holding the sessions mutex across a (possibly slow) client write.
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(OUTBOUND_QUEUE_CAP);
    // Retired sessions keep their session map entry out, so a stale sender
    // snapshot somewhere cannot re-deliver to a recycled identity. The
    // writer exits when all senders are dropped OR when the fallible
    // `alive` flag below flips to false (e.g. this connection's session
    // was evicted by a same-agent reconnect and its stream dropped).
    let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
    if let Ok(writer_stream) = stream.try_clone() {
        let alive_writer = Arc::clone(&alive);
        std::thread::spawn(move || writer_task(writer_stream, rx, alive_writer));
    }

    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(e) => {
            log::error!("failed to clone stream: {}", e);
            alive.store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        }
    };

    // Per-connection auth state. Set to true only after a successful
    // `initialize` (valid token). Every non-`initialize` method is rejected
    // with -32600 until authenticated. Mirrors the MCP server's auth gate
    // (mcp.rs ConnectionHandler::authenticated): without this, any local
    // process that can reach the port could inject notifications/status
    // attributed to arbitrary agents.
    let mut authenticated = false;

    // Capped line reader: bound each line at MAX_AGENT_LINE_BYTES so a
    // misbehaving agent streaming megabytes without a newline cannot force
    // unbounded allocation. Using read_into a reusable buffer + read_until
    // (rather than BufRead::lines()) is what lets us enforce the cap before
    // the full line is materialized.
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    // The session this connection registered via `initialize`. Cleanup is
    // keyed by session id (not peer_addr) so a reconnecting agent cannot
    // remove a duplicate/ recycled session attached to another socket.
    let mut conn_session_id: Option<String> = None;
    'conn: loop {
        buf.clear();
        let mut total: usize = 0;
        let line_result = loop {
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break None, // EOF — peer closed.
                Ok(n) => {
                    total += n;
                    if total > MAX_AGENT_LINE_BYTES {
                        log::warn!(
                            "Agent comms: disconnecting {} — line exceeded {} bytes",
                            peer,
                            MAX_AGENT_LINE_BYTES
                        );
                        // Drop the connection; the oversized line is
                        // discarded. Exit through the same cleanup path as
                        // a normal close so the session entry, pending
                        // inputs, and the renderer's `agents:disconnected`
                        // are all handled.
                        break 'conn;
                    }
                    if buf.last() == Some(&b'\n') {
                        // Complete line.
                        let line = String::from_utf8_lossy(&buf).to_string();
                        break Some(line);
                    }
                    // else: partial read, keep accumulating.
                }
                Err(_) => break None,
            }
        };
        let line = match line_result {
            Some(l) => l,
            None => {
                // EOF or read error: the line-based protocol requires every
                // message to be newline-terminated, so any leftover bytes are
                // an unterminated final line. Log metadata (never content —
                // it may contain prompts or credentials) before discarding.
                if !buf.is_empty() {
                    log::warn!(
                        "Agent comms: discarding {} unterminated trailing bytes at EOF from {}",
                        buf.len(),
                        peer
                    );
                }
                break;
            }
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let msg: AgentMessage = match serde_json::from_str(&trimmed) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if let Err(reason) = validate_agent_message(&msg) {
            log::warn!("Agent comms: rejecting invalid message fields");
            if let Some(id) = msg.id.clone() {
                send_to_socket(
                    &stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32600,
                            "message": reason,
                        }
                    }),
                );
            }
            continue;
        }

        // Auth gate: reject every non-initialize method when not authenticated.
        if msg.method != "initialize" && !authenticated {
            // The method field is unauthenticated input and may be very large
            // or contain terminal-control characters. Log only its size so a
            // local client cannot forge or bloat application logs.
            log::warn!(
                "Agent comms: rejecting unauthenticated method ({} bytes) from {}",
                msg.method.len(),
                peer
            );
            if msg.id.is_some() {
                send_to_socket(
                    &stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32600,
                            "message": "Unauthenticated: initialize required",
                        }
                    }),
                );
            }
            continue;
        }

        // initialize is the only method that may run while unauthenticated.
        if msg.method == "initialize" {
            if let Some(session_id) = handle_initialize(
                &stream,
                msg,
                &sessions,
                &pending_input,
                &token,
                &event_emitter,
                &tx,
            ) {
                authenticated = true;
                conn_session_id = Some(session_id);
                log::info!("Agent comms: client authenticated from {}", peer);
            }
            continue;
        }

        handle_incoming_message(&stream, msg, &sessions, &pending_input, &event_emitter);
    }

    cleanup_connection(
        conn_session_id.as_deref(),
        &sessions,
        &pending_input,
        &event_emitter,
    );
    // Release the outbound queue: dropping this thread's `tx` (plus the
    // session-map copy, removed by cleanup) lets the writer task exit, and
    // the flag covers the case where inserts raced ahead of cleanup.
    alive.store(false, std::sync::atomic::Ordering::SeqCst);
    drop(tx);
    log::info!("Agent comms: connection closed from {}", peer);
}

fn handle_incoming_message(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &EventEmitter,
) {
    // NOTE: `initialize` is handled (and auth-gated) in the connection loop
    // before this function is reached; only post-auth methods dispatch here.
    match msg.method.as_str() {
        "notifications/message" => handle_notification(stream, msg, sessions, event_emitter),
        "agents/status" => handle_status(stream, msg, sessions, event_emitter),
        "agents/requestInput" => {
            handle_request_input(stream, msg, sessions, pending_input, event_emitter)
        }
        "agents/heartbeat" => handle_heartbeat(stream, msg, sessions),
        _ => {
            if msg.id.is_some() {
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {}", msg.method),
                        }
                    }),
                );
            }
        }
    }
}

fn handle_initialize(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    token: &str,
    event_emitter: &EventEmitter,
    tx: &SyncSender<Vec<u8>>,
) -> Option<String> {
    let incoming_token = msg
        .params
        .get("data")
        .and_then(|d| d.get("token"))
        .and_then(|t| t.as_str());

    if incoming_token != Some(token) {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "error": {
                    "code": -32600,
                    "message": "Invalid or missing auth token",
                }
            }),
        );
        return None;
    }

    let session_id = generate_uuid();
    let data = msg.params.get("data").cloned().unwrap_or_default();
    let plugin_id = data
        .get("pluginId")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let agent_id = data
        .get("agentId")
        .and_then(|v| v.as_str())
        .unwrap_or(&format!("agent-{}", &session_id[..8]))
        .to_string();

    let connected_at = now_ms();
    let session = AgentSession {
        id: session_id.clone(),
        plugin_id: plugin_id.clone(),
        agent_id: agent_id.clone(),
        connected_at,
        last_activity_at: connected_at,
        status: SessionStatus::Active,
    };

    let peer_addr = stream.peer_addr().ok();
    let internal = SessionInternal {
        session: session.clone(),
        sender: tx.clone(),
        peer_addr,
    };

    // Evict stale sessions for the same stable identity before inserting.
    // `peer_addr` (client ephemeral port) cannot recognize a reconnect from
    // the same agent, so eviction is keyed on `(plugin_id, agent_id)`. An
    // agent that *defaults* its agent_id (`agent-<random>`) effectively
    // claims a fresh identity each reconnect and relies on socket EOF plus
    // `cleanup_connection` to retire its old session.
    //
    // Collect the sessions to evict inside the lock, then insert the new
    // session; all cross-lock cleanup (pending-input drop, renderer events)
    // runs after both locks are released so we never take `pending_input`
    // while holding `sessions` — any handler doing the reverse cannot
    // deadlock with us.
    let evicted = if let Ok(mut map) = sessions.lock() {
        let evict_ids: Vec<String> = map
            .values()
            .filter(|existing| {
                existing.session.plugin_id == plugin_id && existing.session.agent_id == agent_id
            })
            .map(|existing| existing.session.id.clone())
            .collect();
        let mut evicted = Vec::with_capacity(evict_ids.len());
        for id in evict_ids {
            if let Some(old) = map.remove(&id) {
                evicted.push(old.session);
            }
        }
        if !evicted.is_empty() {
            log::info!(
                "Agent comms: evicting {} stale session(s) for same agent identity",
                evicted.len()
            );
        }
        map.insert(session_id.clone(), internal);
        evicted
    } else {
        Vec::new()
    };

    if !evicted.is_empty() {
        // Dropping an evicted session's pending-input senders wakes its
        // `recv_timeout` with `Disconnected`, so an agent that reconnected
        // while an input dialog was open gets a prompt cancellation instead
        // of hanging for the full 30s timeout.
        if let Ok(mut pending) = pending_input.lock() {
            let ids: std::collections::HashSet<&str> =
                evicted.iter().map(|s| s.id.as_str()).collect();
            pending.retain(|_, entry| !ids.contains(entry.session_id.as_str()));
        }
        for old in &evicted {
            emit_to_renderer(
                event_emitter,
                "agents:disconnected",
                &serde_json::json!({
                    "sessionId": old.id,
                    "agentId": old.agent_id,
                    "pluginId": old.plugin_id,
                }),
            );
        }
    }

    send_to_socket(
        stream,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": msg.id,
            "result": {
                "sessionId": session_id,
                "agentId": agent_id,
                "protocolVersion": "1.0.0",
                "capabilities": ["notification", "status_update", "input_request", "error", "completion"],
            }
        }),
    );

    emit_to_renderer(
        event_emitter,
        "agents:connected",
        &serde_json::json!({
            "sessionId": session_id,
            "pluginId": plugin_id,
            "agentId": agent_id,
            "connectedAt": connected_at,
        }),
    );

    log::info!(
        "Agent connected: session_id_bytes={} plugin_id_bytes={} agent_id_bytes={}",
        session.id.len(),
        session.plugin_id.len(),
        session.agent_id.len()
    );
    Some(session_id)
}

fn handle_notification(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    event_emitter: &EventEmitter,
) {
    let agent_id = msg.params.get("agentId").and_then(|v| v.as_str());
    let session = agent_id.and_then(|aid| find_session_by_agent_id(sessions, aid));

    if let Some(aid) = agent_id {
        update_activity_by_agent_id(sessions, aid);
    }

    let level = msg
        .params
        .get("level")
        .and_then(|v| v.as_str())
        .unwrap_or("info");
    let status = if level == "needs_input" {
        SessionStatus::WaitingInput
    } else {
        SessionStatus::Active
    };

    if let Some(ref s) = session {
        update_session_status(sessions, &s.id, status.clone());
        emit_to_renderer(
            event_emitter,
            "agents:statusUpdate",
            &serde_json::json!({
                "sessionId": s.id,
                "agentId": s.agent_id,
                "status": status,
                "data": msg.params.get("data"),
            }),
        );
    }

    // Preserve the high-fidelity plugin notification as a separate event.
    // The Tauri adapter resolves its agentId to paneId and routes it through
    // the shared NotificationService, so plugin notifications receive the
    // same in-app/native treatment as passive tracker notifications.
    emit_to_renderer(
        event_emitter,
        "agents:notification",
        &serde_json::json!({
            "sessionId": session.as_ref().map(|s| s.id.as_str()),
            "agentId": session.as_ref().map(|s| s.agent_id.as_str()),
            "level": level,
            "title": msg.params.get("title").and_then(|v| v.as_str()).unwrap_or("Agent Notification"),
            "message": msg.params.get("message").and_then(|v| v.as_str()).unwrap_or(""),
            "data": msg.params.get("data"),
            "timestamp": now_ms(),
        }),
    );

    // Notification payloads can contain prompts, terminal output, paths, or
    // credentials supplied by an agent. The renderer receives the structured
    // event above; application logs must remain metadata-only.
    log::info!(
        "[agent notification] received level_bytes={} agent_id_bytes={}",
        level.len(),
        session.as_ref().map(|s| s.agent_id.len()).unwrap_or(0)
    );

    if msg.id.is_some() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "result": { "acknowledged": true }
            }),
        );
    }
}

fn handle_status(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    event_emitter: &EventEmitter,
) {
    let session = find_session_by_stream(sessions, stream);
    if let Some(ref s) = session {
        update_activity_by_session_id(sessions, &s.id);
        let new_status = msg
            .params
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("active");

        let status_enum = match new_status {
            "waiting_input" => SessionStatus::WaitingInput,
            "idle" => SessionStatus::Idle,
            "disconnected" => SessionStatus::Disconnected,
            _ => SessionStatus::Active,
        };

        update_session_status(sessions, &s.id, status_enum.clone());

        emit_to_renderer(
            event_emitter,
            "agents:statusUpdate",
            &serde_json::json!({
                "sessionId": s.id,
                "agentId": s.agent_id,
                "status": new_status,
                "data": msg.params.get("data"),
            }),
        );

        if new_status == "waiting_input" {
            if let Some(prompt) = msg.params.get("prompt").and_then(|v| v.as_str()) {
                // Prompts may contain private workspace context or secrets.
                // Keep this diagnostic metadata-only; the renderer receives
                // the prompt through the structured event above.
                let _ = prompt;
                log::info!(
                    "[agent status] waiting_input agent_id_bytes={}",
                    s.agent_id.len()
                );
            }
        }
    }

    if msg.id.is_some() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "result": { "acknowledged": true }
            }),
        );
    }
}

pub(super) fn handle_request_input(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &EventEmitter,
) {
    let session = find_session_by_stream(sessions, stream);
    let Some(session) = session else {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "error": {
                    "code": -32000,
                    "message": "Not initialized",
                }
            }),
        );
        return;
    };
    update_activity_by_session_id(sessions, &session.id);
    update_session_status(sessions, &session.id, SessionStatus::WaitingInput);

    let request_id = msg
        .params
        .get("requestId")
        .and_then(|v| v.as_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= MAX_AGENT_ID_BYTES
                && !value.chars().any(|c| c.is_control())
        })
        .unwrap_or(&generate_uuid())
        .to_string();

    let prompt = msg
        .params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Titles and prompts are agent-provided text and may contain private
    // workspace context or credentials. Keep the log metadata-only.
    log::info!(
        "[agent input_request] received request_id_bytes={} agent_id_bytes={}",
        request_id.len(),
        session.agent_id.len()
    );

    emit_to_renderer(
        event_emitter,
        "agents:inputRequested",
        &serde_json::json!({
            "sessionId": session.id,
            "agentId": session.agent_id,
            "requestId": request_id,
            "prompt": prompt,
            "message": prompt,
        }),
    );

    if msg.id.is_some() {
        let (input_tx, input_rx) = std::sync::mpsc::sync_channel::<String>(1);

        {
            let mut map = match pending_input.lock() {
                Ok(g) => g,
                Err(_) => {
                    log::error!("Agent comms: pending_input lock poisoned");
                    return;
                }
            };
            // Reject duplicate request ids instead of overwriting: inserting
            // over an existing entry would silently drop the original sender
            // and wake its waiter with a spurious `Disconnected`/cancelled.
            match map.entry(request_id.clone()) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(PendingInput {
                        session_id: session.id.clone(),
                        sender: input_tx,
                    });
                }
                std::collections::hash_map::Entry::Occupied(_) => {
                    drop(map);
                    send_to_socket(
                        stream,
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": msg.id,
                            "error": {
                                "code": -32000,
                                "message": "input request id already in progress",
                            }
                        }),
                    );
                    update_session_status(sessions, &session.id, SessionStatus::Active);
                    emit_to_renderer(
                        event_emitter,
                        "agents:statusUpdate",
                        &serde_json::json!({
                            "sessionId": session.id,
                            "agentId": session.agent_id,
                            "status": "active",
                        }),
                    );
                    return;
                }
            }
        }

        match input_rx.recv_timeout(INPUT_REQUEST_TIMEOUT) {
            Ok(response) => {
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "result": { "input": response }
                    }),
                );
                update_session_status(sessions, &session.id, SessionStatus::Active);
                update_activity_by_session_id(sessions, &session.id);
                emit_to_renderer(
                    event_emitter,
                    "agents:statusUpdate",
                    &serde_json::json!({
                        "sessionId": session.id,
                        "agentId": session.agent_id,
                        "status": "active",
                    }),
                );
            }
            Err(RecvTimeoutError::Timeout) => {
                // Remove the stale request so it does not leak.
                if let Ok(mut map) = pending_input.lock() {
                    map.remove(&request_id);
                }
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32000,
                            "message": "Input request timed out",
                        }
                    }),
                );
                // The agent is no longer waiting for this input; restore
                // the session state so the UI badge does not stick on
                // `waiting_input` forever.
                update_session_status(sessions, &session.id, SessionStatus::Active);
                emit_to_renderer(
                    event_emitter,
                    "agents:statusUpdate",
                    &serde_json::json!({
                        "sessionId": session.id,
                        "agentId": session.agent_id,
                        "status": "active",
                    }),
                );
            }
            Err(RecvTimeoutError::Disconnected) => {
                // Sender was dropped, most likely by cancel_input_request
                // or by cleanup_connection on agent disconnect.
                send_to_socket(
                    stream,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg.id,
                        "error": {
                            "code": -32000,
                            "message": "Input request cancelled",
                        }
                    }),
                );
                // On user-driven cancellation the session is still alive and
                // returns to `Active`; when the disconnect came from
                // `cleanup_connection` the session entry is already gone and
                // this status update is a no-op on an absent id.
                update_session_status(sessions, &session.id, SessionStatus::Active);
                emit_to_renderer(
                    event_emitter,
                    "agents:statusUpdate",
                    &serde_json::json!({
                        "sessionId": session.id,
                        "agentId": session.agent_id,
                        "status": "active",
                    }),
                );
            }
        }
    }
}

fn handle_heartbeat(
    stream: &TcpStream,
    msg: AgentMessage,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
) {
    let session = find_session_by_stream(sessions, stream);
    if let Some(ref s) = session {
        update_activity_by_session_id(sessions, &s.id);
    }

    if msg.id.is_some() {
        send_to_socket(
            stream,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg.id,
                "result": { "ts": now_ms() }
            }),
        );
    }
}

fn find_session_by_agent_id(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    agent_id: &str,
) -> Option<AgentSession> {
    let guard = match sessions.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    guard
        .values()
        .find(|s| s.session.agent_id == agent_id)
        .map(|s| s.session.clone())
}

fn find_session_by_stream(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    stream: &TcpStream,
) -> Option<AgentSession> {
    let peer_addr = match stream.peer_addr() {
        Ok(addr) => Some(addr),
        Err(_) => return None,
    };
    let guard = match sessions.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    guard
        .values()
        .find(|s| s.peer_addr == peer_addr)
        .map(|s| s.session.clone())
}

fn update_activity_by_agent_id(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    agent_id: &str,
) {
    if let Ok(mut guard) = sessions.lock() {
        for internal in guard.values_mut() {
            if internal.session.agent_id == agent_id {
                internal.session.last_activity_at = now_ms();
                break;
            }
        }
    }
}

fn update_activity_by_session_id(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    session_id: &str,
) {
    if let Ok(mut guard) = sessions.lock() {
        if let Some(internal) = guard.get_mut(session_id) {
            internal.session.last_activity_at = now_ms();
        }
    }
}

fn update_session_status(
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    session_id: &str,
    status: SessionStatus,
) {
    if let Ok(mut guard) = sessions.lock() {
        if let Some(internal) = guard.get_mut(session_id) {
            internal.session.status = status;
        }
    }
}

fn cleanup_connection(
    session_id: Option<&str>,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &EventEmitter,
) {
    // Keyed by the session this connection registered — `peer_addr` would
    // collide with sibling reconnects sharing neither socket nor identity,
    // and using it here could tear down a *different* agent's session.
    //
    // Remove the session and drop its pending-input senders in one pass:
    // session removal takes the `sessions` lock, then pending pruning takes
    // `pending_input` after the first lock is released, keeping this path
    // incompatible with any handler nesting these locks the other way.
    let Some(session_id) = session_id else {
        return;
    };
    let session = {
        let Ok(mut guard) = sessions.lock() else {
            return;
        };
        guard.remove(session_id).map(|internal| internal.session)
    };
    if let Some(s) = session {
        if let Ok(mut pending) = pending_input.lock() {
            pending.retain(|_, entry| entry.session_id != s.id);
        }

        emit_to_renderer(
            event_emitter,
            "agents:disconnected",
            &serde_json::json!({
                "sessionId": s.id,
                "agentId": s.agent_id,
                "pluginId": s.plugin_id,
            }),
        );

        log::info!("Agent disconnected: agent_id_bytes={}", s.agent_id.len());
    }
}
