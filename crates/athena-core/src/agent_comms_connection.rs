//! Agent-comms TCP connection lifecycle and message dispatch helpers.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};

use super::{
    generate_uuid, now_ms, AgentMessage, AgentSession, PendingInput, SessionInternal,
    SessionStatus, INPUT_REQUEST_TIMEOUT, MAX_AGENT_LINE_BYTES,
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
    log::debug!("[agent-comms] {} -> {}", channel, data);
}

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

    let (tx, _rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1024);
    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(e) => {
            log::error!("failed to clone stream: {}", e);
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
    loop {
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
                        // Drop the connection; the oversized line is discarded.
                        return;
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
            None => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let msg: AgentMessage = match serde_json::from_str(&trimmed) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Auth gate: reject every non-initialize method when not authenticated.
        if msg.method != "initialize" && !authenticated {
            log::warn!(
                "Agent comms: rejecting unauthenticated '{}' from {}",
                msg.method,
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
            if handle_initialize(&stream, msg, &sessions, &token, &event_emitter, &tx) {
                authenticated = true;
                log::info!("Agent comms: client {} authenticated", peer);
            }
            continue;
        }

        handle_incoming_message(&stream, msg, &sessions, &pending_input, &event_emitter);
    }

    cleanup_connection(&stream, &sessions, &pending_input, &event_emitter);
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
    token: &str,
    event_emitter: &EventEmitter,
    tx: &SyncSender<Vec<u8>>,
) -> bool {
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
        return false;
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

    if let Ok(mut map) = sessions.lock() {
        // Evict any existing session from the same peer address to prevent
        // memory leaks when a client reconnects without proper cleanup.
        if let Some(addr) = peer_addr {
            map.retain(|_, existing| existing.peer_addr != Some(addr));
        }
        map.insert(session_id.clone(), internal);
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
        "Agent connected: session={} plugin={} agent={}",
        session.id,
        session.plugin_id,
        session.agent_id
    );
    true
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

    let notif = serde_json::json!({
        "type": level,
        "title": msg.params.get("title").and_then(|v| v.as_str()).unwrap_or("Agent Notification"),
        "message": msg.params.get("message").and_then(|v| v.as_str()).unwrap_or(""),
        "source": session.as_ref().map(|s| &s.plugin_id).unwrap_or(&"unknown".into()),
        "agentId": session.as_ref().map(|s| &s.agent_id),
        "data": msg.params.get("data"),
        "timestamp": now_ms(),
    });

    log::info!("[agent notification] {}", notif);

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
                log::info!(
                    "[agent status] waiting_input for agent={}: {}",
                    s.agent_id,
                    prompt
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
    if session.is_none() {
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
    }

    let session = session.unwrap();
    update_activity_by_session_id(sessions, &session.id);
    update_session_status(sessions, &session.id, SessionStatus::WaitingInput);

    let request_id = msg
        .params
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or(&generate_uuid())
        .to_string();

    let prompt = msg
        .params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let title = msg
        .params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Input Request");

    log::info!(
        "[agent input_request] requestId={} agent={} title={}",
        request_id,
        session.agent_id,
        title
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
            map.insert(
                request_id.clone(),
                PendingInput {
                    session_id: session.id.clone(),
                    sender: input_tx,
                },
            );
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
    stream: &TcpStream,
    sessions: &Arc<Mutex<HashMap<String, SessionInternal>>>,
    pending_input: &Arc<Mutex<HashMap<String, PendingInput>>>,
    event_emitter: &EventEmitter,
) {
    let peer_addr = match stream.peer_addr() {
        Ok(addr) => Some(addr),
        Err(_) => return,
    };
    let session = {
        let guard = match sessions.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        guard
            .values()
            .find(|s| s.peer_addr == peer_addr)
            .map(|s| s.session.clone())
    };

    if let Some(s) = session {
        if let Ok(mut guard) = sessions.lock() {
            guard.retain(|_, internal| internal.session.id != s.id);
        }

        // Drop any pending input senders that belong to this session.
        // Removing them wakes the corresponding `recv_timeout` with
        // `Disconnected`, so the agent's input handler thread can exit
        // immediately instead of waiting the full 30s for the timeout.
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

        log::info!("Agent disconnected: agent={}", s.agent_id);
    }
}
