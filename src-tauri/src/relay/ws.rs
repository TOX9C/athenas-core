//! WebSocket upgrade handler. Holds the `RelayCtx` (an `AppHandle`) and
//! routes each incoming `invoke`/`listen`/`unlisten` frame: invoke calls go
//! to `dispatch::dispatch`; listen registers a real Tauri event listener
//! whose forwarded payload is pushed back to the phone over a writer task.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tauri::{Emitter, EventId, Listener, Manager};
use tokio::sync::{mpsc, Notify};

pub const MAX_RELAY_CONNECTIONS: usize = 8;
/// Tiny budget for sockets that have passed token auth but are still awaiting
/// the human pairing approval. Kept well below `MAX_RELAY_CONNECTIONS` so an
/// unapproved peer can't exhaust the real slots (see `handle_upgrade`).
const MAX_PENDING_RELAY_CONNECTIONS: usize = 2;
const MAX_RELAY_FRAMES_PER_WINDOW: usize = 240;
const RELAY_FRAME_WINDOW: Duration = Duration::from_secs(10);
const MAX_RELAY_FRAME_BYTES: usize = 64 * 1024;
const RELAY_WRITER_QUEUE: usize = 1024;
/// How long the desktop operator has to approve a pairing request before the
/// WebSocket upgrade is denied. Generous by design — they may be away from the
/// machine when the phone connects.
const PAIRING_TIMEOUT: Duration = Duration::from_secs(120);
static ACTIVE_RELAY_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
/// Track relay `pty:raw:<pane>` subscriptions in shared state so the PTY read
/// loop only pays for the base64 event path while a phone is listening.
/// `delta` is +1 (listen) or -1 (unlisten/disconnect cleanup).
fn note_raw_listener_event(app: &tauri::AppHandle, event_name: &str, delta: isize) {
    let Some(pane_id) = event_name.strip_prefix("pty:raw:") else {
        return;
    };
    if pane_id.is_empty() {
        return;
    }
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let mut counts = state.relay_raw_subscribers.lock();
    if delta > 0 {
        *counts.entry(pane_id.to_string()).or_insert(0) += 1;
    } else if let Some(count) = counts.get_mut(pane_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            counts.remove(pane_id);
        }
    }
}
static PENDING_RELAY_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

use super::dispatch;
use super::RelayCtx;

/// Axum handler for `GET /ws` — authenticates the bearer token before
/// upgrading. The token is carried by the explicit QR/deep link shown by the
/// desktop, so enabling the relay grants a specific capability instead of
/// exposing the command bridge to every LAN peer.
pub async fn handle_upgrade(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(ctx): State<RelayCtx>,
) -> axum::response::Response {
    let expected_protocol = format!("athena-relay.{}", ctx.token);
    if !auth_subprotocol_matches(&headers, &expected_protocol) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Two-stage slot budget: a socket awaiting pairing approval holds a small
    // "pending" slot, not a real connection slot. A LAN peer that opens
    // sockets and never answers the pairing prompt could otherwise pin all
    // MAX_RELAY_CONNECTIONS slots for the full PAIRING_TIMEOUT and block the
    // legitimate phone. The real slot is only acquired after approval.
    let Some(pending_guard) = try_acquire_pending_connection() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };

    // Human-in-the-loop pairing confirmation. The token proves the device saw
    // the QR/deep link, but a LAN peer that sniffed the URL fragment could
    // replay it. Require the desktop operator to approve each connection
    // before the command/event surface is granted. The upgrade future awaits
    // a oneshot that the desktop resolves via `relay_pairing_respond`.
    let request_id = uuid::Uuid::new_v4().simple().to_string();
    let peer = peer_description(&headers);
    let (approve_tx, approve_rx) = tokio::sync::oneshot::channel::<bool>();
    {
        let state = ctx.app_handle.state::<crate::state::AppState>();
        state
            .relay_pairing_requests
            .lock()
            .insert(request_id.clone(), approve_tx);
    }
    let payload = serde_json::json!({ "requestId": request_id, "peer": peer });
    if let Err(e) = ctx
        .app_handle
        .emit("relay:pairingRequest", payload.to_string())
    {
        log::warn!("[relay] failed to emit pairing request: {e}");
    }

    let ctx_for_cleanup = ctx.clone();
    let ctx_for_session = ctx.clone();
    let request_id_for_cleanup = request_id.clone();
    ws.protocols([expected_protocol])
        .on_upgrade(move |mut socket| async move {
            let approved = tokio::time::timeout(PAIRING_TIMEOUT, approve_rx)
                .await
                .ok()
                .and_then(|res| res.ok())
                .unwrap_or(false);
            {
                let state = ctx_for_cleanup.app_handle.state::<crate::state::AppState>();
                state
                    .relay_pairing_requests
                    .lock()
                    .remove(&request_id_for_cleanup);
            }
            // The approval window is over — release the pending slot. An
            // approved connection now competes for a real slot; if every real
            // slot is taken by live sessions, deny rather than exceed the cap.
            drop(pending_guard);
            if approved {
                let Some(connection_guard) = try_acquire_connection() else {
                    log::warn!(
                        "[relay] all {} connection slots in use; denying approved pairing ({request_id_for_cleanup})",
                        MAX_RELAY_CONNECTIONS
                    );
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                };
                log::info!("[relay] pairing approved ({request_id_for_cleanup})");
                session_loop(socket, ctx_for_session, connection_guard).await;
            } else {
                log::warn!("[relay] pairing denied or timed out ({request_id_for_cleanup})");
                let _ = socket.send(Message::Close(None)).await;
            }
        })
}

/// Human-readable description of the connecting peer for the desktop pairing
/// prompt. Uses the User-Agent (browser/OS) because the relay runs plaintext
/// HTTP without a TLS client cert to identify the device. The remote IP is not
/// reliably available without wiring axum's `ConnectInfo` through the router.
fn peer_description(headers: &HeaderMap) -> String {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown device".to_string())
}

/// One connected phone session. Reads frames, dispatches `invoke` messages,
/// wires `listen` to a real Tauri event listener, and forwards emitted
/// events back through the socket.
///
/// Architecture:
///   - The socket is split into sink + stream halves. The reader half drives
///     the main loop; responses and forwarded events are written through a
///     bounded `mpsc` channel fed to a dedicated writer task. This keeps
///     session state on one task while writes are non-blocking.
///   - `listen` registers `app.listen_any(event, handler)`. The handler
///     pushes `{t:"event", event, payload}` to the writer channel. The
///     returned `EventId` is stored keyed by the shim-assigned listener id.
///   - `unlisten` removes the registered `EventId`. On disconnect we drop
///     every registered listener so no dangling callbacks outlive the phone.
async fn session_loop(socket: WebSocket, ctx: RelayCtx, _connection: RelayConnectionGuard) {
    log::info!("[relay] ws session opened");

    let app = ctx.app_handle.clone();
    let (mut sink, mut stream) = socket.split();
    // Event callbacks run outside this async session task. The bounded queue
    // prevents a slow phone from turning terminal output into unbounded memory
    // growth; event frames are dropped when the phone cannot keep up.
    let (tx, mut rx) = mpsc::channel::<String>(RELAY_WRITER_QUEUE);
    let overloaded = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let overload_notify = Arc::new(Notify::new());
    let writer_overload_notify = Arc::clone(&overload_notify);
    let mut frame_window_started = Instant::now();
    let mut frames_in_window = 0usize;

    // Writer task: drains the mpsc channel and pushes frames to the sink.
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink.send(Message::Text(frame)).await.is_err() {
                writer_overload_notify.notify_one();
                break;
            }
        }
        // Close the sink cleanly so the peer sees the FIN.
        let _ = sink.send(Message::Close(None)).await;
    });

    // Registered Tauri event listeners, keyed by the shim-side listener id
    // (so `unlisten` and disconnect cleanup can find them). Guarded by a
    // mutex because Tauri's event handlers fire from arbitrary threads. The
    // event name is retained so `pty:raw:<pane>` subscriber counts can be
    // decremented on unlisten/disconnect.
    let listeners: Arc<Mutex<HashMap<String, (EventId, String)>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Per-connection registry of panes this phone spawned. Combined with the
    // desktop's shared-pane set (read live via `shared_pane_ids`) to decide
    // which terminal read/write surface the phone may access. The pairing
    // token gates the connection; per-pane sharing gates the content.
    let owned_pane_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    loop {
        let frame = tokio::select! {
            _ = overload_notify.notified() => {
                log::warn!("[relay] closing slow client after writer backpressure");
                break;
            }
            frame = stream.next() => match frame {
                Some(frame) => frame,
                None => break,
            },
        };
        if overloaded.load(Ordering::Acquire) {
            log::warn!("[relay] closing slow client after writer backpressure");
            break;
        }
        let frame = match frame {
            Ok(f) => f,
            Err(e) => {
                log::warn!("[relay] ws recv error: {e}");
                break;
            }
        };
        let text = match frame {
            Message::Text(t) => t,
            Message::Close(_) => {
                log::info!("[relay] ws session closed");
                break;
            }
            // Binary frames are ignored — the shim only sends text.
            _ => continue,
        };

        if text.len() > MAX_RELAY_FRAME_BYTES {
            log::warn!("[relay] oversized ws frame rejected");
            break;
        }

        let msg = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[relay] malformed ws frame: {e}");
                continue;
            }
        };

        if frame_window_started.elapsed() >= RELAY_FRAME_WINDOW {
            frame_window_started = Instant::now();
            frames_in_window = 0;
        }
        frames_in_window += 1;
        if frames_in_window > MAX_RELAY_FRAMES_PER_WINDOW {
            log::warn!("[relay] websocket frame-rate limit exceeded");
            break;
        }

        let kind = msg.get("t").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "invoke" => {
                let id = msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cmd = msg
                    .get("cmd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = msg.get("args").cloned().unwrap_or(serde_json::Value::Null);
                let pane_id = dispatch::pane_id_of(&cmd, &args);
                // Per-pane share gate: pane-scoped terminal read/write is
                // authorized only for panes this phone spawned OR the desktop
                // explicitly shared (default off). Status-only pane queries
                // (has_session/agent_info/…) remain ungated — they leak no
                // terminal content and the mobile attach flow pre-checks them.
                // `dispatch::authorize_command` is the single, unit-tested
                // authorization boundary for every relay invoke frame.
                let authorization = {
                    let owned = owned_pane_ids.lock();
                    dispatch::authorize_command(&cmd, &args, &owned, &shared_pane_ids(&app))
                };
                let result = match authorization {
                    Ok(()) => dispatch::dispatch(&ctx, &cmd, args).await,
                    Err(reason) => Err(reason),
                };
                // Record the pane id this phone spawned/killed, but only after
                // a successful dispatch. pty_spawn/pty_spawn_agent return
                // Ok(()) for duplicate ids without re-spawning (pty.rs:187),
                // so inserting here covers both initial spawn AND reconnect
                // re-registration of an already-running pane in one place.
                if result.is_ok() {
                    if let Some(pane) = &pane_id {
                        match cmd.as_str() {
                            "pty_spawn" | "pty_spawn_agent" => {
                                owned_pane_ids.lock().insert(pane.clone());
                            }
                            "pty_kill" => {
                                owned_pane_ids.lock().remove(pane);
                            }
                            _ => {}
                        }
                    }
                }
                let (ok, value) = match &result {
                    Ok(v) => (true, v.clone()),
                    Err(e) => (false, serde_json::Value::String(e.clone())),
                };
                let resp = if ok {
                    serde_json::json!({ "t": "resp", "id": id, "ok": true, "result": value })
                } else {
                    serde_json::json!({ "t": "resp", "id": id, "ok": false, "error": value })
                };
                if tx.try_send(resp.to_string()).is_err() {
                    log::warn!("[relay] response queue full; closing slow client");
                    break;
                }
            }
            "listen" => {
                let lid = msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let event = msg
                    .get("event")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if event.is_empty() || !event_allowed(&event) {
                    log::warn!("[relay] rejected event subscription: {event}");
                    continue;
                }
                let listener_limit_reached = {
                    let guard = listeners.lock();
                    guard.len() >= 128 && !guard.contains_key(&lid)
                };
                if listener_limit_reached {
                    log::warn!("[relay] listener limit reached");
                    continue;
                }
                log::debug!("[relay] listen register: {event} ({lid})");
                let tx_clone = tx.clone();
                let overloaded_for_event = Arc::clone(&overloaded);
                let event_overload_notify = Arc::clone(&overload_notify);
                let event_name = event.clone();
                // The forwarding closure moves `event_name`; keep a second
                // copy for the listener registry / subscriber counters.
                let registry_event_name = event_name.clone();
                let app_for_listen = app.clone();
                // The shared-pane read inside the forwarding closure needs its
                // own AppHandle clone: `app_for_listen` is the `listen_any`
                // receiver, so the closure can't also move it.
                let app_for_filter = app_for_listen.clone();
                let owned_for_filter = Arc::clone(&owned_pane_ids);
                // Whether this event family carries terminal/console payloads
                // keyed by a pane id the phone must own. Non-terminal events
                // (notifications, plugin registry, fs:change, agent status)
                // are forwarded unfiltered — they are not per-pane streams.
                let is_terminal_stream = matches!(
                    event_name.as_str(),
                    "output-capture:batch" | "terminal:data" | "terminal:exit"
                ) || event_name.starts_with("pty:raw:");
                let event_name_in_closure = event_name.clone();
                let eid = app_for_listen.listen_any(event_name.clone(), move |ev| {
                    let event_name = event_name_in_closure.as_str();
                    let payload = ev.payload().to_string();
                    // Terminal-stream forwarding is gated on the phone owning
                    // the pane whose id the event carries OR the desktop having
                    // shared it. This is the real credential-leak closure: even
                    // though listen_any fires for every desktop pane, a paired
                    // phone only receives output/exit events for panes it
                    // spawned or that the desktop explicitly shared.
                    if is_terminal_stream {
                        // Prefer the session-scoped channel suffix for raw
                        // PTY events. Those payloads intentionally contain
                        // only the base64 data, so looking only inside JSON
                        // would silently drop every relay raw frame.
                        let owns = terminal_event_pane_id(event_name, &payload)
                            .map(|id| {
                                owned_for_filter.lock().contains(&id)
                                    || shared_pane_ids(&app_for_filter).contains(&id)
                            })
                            .unwrap_or(false);
                        if !owns {
                            return;
                        }
                    }
                    let out = serde_json::json!({
                        "t": "event",
                        "event": event_name,
                        "payload": payload,
                    });
                    // Terminal bytes are stateful: never drop a frame or call
                    // blocking_send from a runtime callback. The unbounded
                    // sender is consumed by the dedicated socket writer.
                    if tx_clone.try_send(out.to_string()).is_err() {
                        // Do not keep a stateful terminal stream alive after a
                        // frame is dropped. The reader observes this flag and
                        // closes the slow client on its next turn.
                        overloaded_for_event.store(true, Ordering::Release);
                        event_overload_notify.notify_one();
                    }
                });
                if let Some(previous) = listeners.lock().insert(lid, (eid, event_name.clone())) {
                    // Re-registering the same shim listener id must not leave
                    // the old Tauri callback alive (reconnects can replay
                    // listener registrations).
                    app.unlisten(previous.0);
                    note_raw_listener_event(&app, &previous.1, -1);
                }
                note_raw_listener_event(&app, &registry_event_name, 1);
            }
            "unlisten" => {
                let lid = msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                log::debug!("[relay] unlisten: ({lid})");
                let eid = listeners.lock().remove(&lid);
                if let Some((id, event_name)) = eid {
                    app.unlisten(id);
                    note_raw_listener_event(&app, &event_name, -1);
                }
            }
            _ => {
                log::warn!("[relay] unknown ws message kind: {kind}");
            }
        }
    }

    // Cleanup: drop every registered listener so no callbacks outlive the
    // phone session, then close the writer channel so the writer task exits.
    let registered: Vec<(EventId, String)> = {
        let mut guard = listeners.lock();
        guard.drain().map(|(_, entry)| entry).collect()
    };
    for (eid, event_name) in registered {
        app.unlisten(eid);
        note_raw_listener_event(&app, &event_name, -1);
    }
    drop(tx);
    let _ = writer.await;

    log::info!("[relay] ws session ended");
}

struct RelayConnectionGuard;

fn try_acquire_connection() -> Option<RelayConnectionGuard> {
    ACTIVE_RELAY_CONNECTIONS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_RELAY_CONNECTIONS).then_some(count + 1)
        })
        .ok()
        .map(|_| RelayConnectionGuard)
}

impl Drop for RelayConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_RELAY_CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Guard for the pre-approval budget (see `MAX_PENDING_RELAY_CONNECTIONS`).
struct PendingRelayConnectionGuard;

fn try_acquire_pending_connection() -> Option<PendingRelayConnectionGuard> {
    PENDING_RELAY_CONNECTIONS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_PENDING_RELAY_CONNECTIONS).then_some(count + 1)
        })
        .ok()
        .map(|_| PendingRelayConnectionGuard)
}

impl Drop for PendingRelayConnectionGuard {
    fn drop(&mut self) {
        PENDING_RELAY_CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn auth_subprotocol_matches(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').any(|item| item.trim() == expected))
        .unwrap_or(false)
}

/// Read the live set of panes the desktop has shared with the mobile mirror.
/// Empty by default; the desktop toggles membership via `relay_set_pane_shared`.
/// This is read on each terminal-stream event (not snapshotted per connection)
/// so the desktop can share/unshare panes while a phone is already paired.
fn shared_pane_ids(app: &tauri::AppHandle) -> HashSet<String> {
    let state = app.state::<crate::state::AppState>();
    let ids: HashSet<String> = state.relay_shared_panes.lock().iter().cloned().collect();
    ids
}

/// Extract the pane ID carried by a terminal event for relay authorization.
/// Session-scoped `pty:raw:<pane>` events use the channel name as their
/// identity because their payload contains only base64 data. Older event
/// families keep their pane ID in the JSON payload for compatibility.
fn terminal_event_pane_id(event_name: &str, payload: &str) -> Option<String> {
    if let Some(pane_id) = event_name.strip_prefix("pty:raw:") {
        return (!pane_id.is_empty()).then(|| pane_id.to_string());
    }

    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("paneId")
                .or_else(|| value.get("sessionId"))
                .or_else(|| value.get("session_id"))
                .and_then(|id| id.as_str())
                .map(str::to_string)
        })
        .or_else(|| {
            // Backward compatibility for older desktop emitters that sent the
            // pane ID as a JSON string rather than an object.
            (event_name == "terminal:exit")
                .then(|| payload.trim().trim_matches('"').to_string())
                .filter(|id| !id.is_empty())
        })
}

/// Events exposed to the mobile mirror. Keep this list narrow: `listen_any`
/// otherwise permits a client to subscribe to unrelated backend events.
fn event_allowed(event: &str) -> bool {
    matches!(
        event,
        "agent:status"
            | "agents:connected"
            | "agents:disconnected"
            | "agents:statusUpdate"
            | "agents:inputRequested"
            | "notifications:new"
            | "notifications:updated"
            | "notifications:dismissed"
            | "notifications:resolved"
            | "notifications:cleared"
            | "output-capture:batch"
            | "output-capture:paneRegistered"
            | "plugin:registryUpdated"
            | "plugin:registered"
            | "plugin:enabled"
            | "plugin:disabled"
            | "plugin:error"
            | "plugin:event"
            | "swarm:stateChange"
            | "athena:stream"
            | "athena:askUser"
            | "athena:planUpdate"
            | "athena:planEvaluated"
            | "terminal:exit"
            // NOTE: event_allowed is an allowlist of event NAMES, while the
            // forwarding callback applies the per-pane workspace/ownership
            // filter. Mobile xterm consumes raw ANSI bytes directly.
            | "pty:raw"
            | "tauri://resize"
    ) || event.starts_with("fs:change:")
        || event.starts_with("pty:raw:")
}

#[cfg(test)]
mod tests {
    use super::{
        auth_subprotocol_matches, event_allowed, peer_description, terminal_event_pane_id,
    };
    use axum::http::{header, HeaderMap, HeaderValue};

    #[test]
    fn requires_the_exact_websocket_subprotocol() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static("athena-relay.secret"),
        );
        assert!(auth_subprotocol_matches(&headers, "athena-relay.secret"));
        assert!(!auth_subprotocol_matches(&headers, "athena-relay.other"));
        assert!(!auth_subprotocol_matches(
            &HeaderMap::new(),
            "athena-relay.secret"
        ));
    }

    #[test]
    fn allows_mobile_workspace_events() {
        assert!(event_allowed("output-capture:batch"));
        assert!(event_allowed("notifications:new"));
        assert!(event_allowed("fs:change:/workspace/file"));
    }

    #[test]
    fn allows_authenticated_raw_pty_streams() {
        // terminal:data remains disabled; session-scoped pty:raw channels are
        // the authenticated live streams consumed by mobile xterm.
        assert!(!event_allowed("terminal:data"));
        assert!(event_allowed("pty:raw"));
        assert!(event_allowed("pty:raw:abc-123"));
        assert!(!event_allowed("pty:rawish:abc-123"));
    }
    #[test]
    fn rejects_arbitrary_event_subscriptions() {
        assert!(!event_allowed("secret:credentials"));
        assert!(!event_allowed("tauri://menu"));
        assert!(!event_allowed(""));
    }

    #[test]
    fn raw_terminal_events_get_pane_id_from_session_scoped_channel() {
        assert_eq!(
            terminal_event_pane_id("pty:raw:pane-7", r#"{"data":"AQI="}"#),
            Some("pane-7".to_string())
        );
        assert_eq!(
            terminal_event_pane_id("pty:raw:", r#"{"data":"AQI="}"#),
            None
        );
    }

    #[test]
    fn terminal_event_pane_id_supports_legacy_payload_shapes() {
        assert_eq!(
            terminal_event_pane_id("terminal:data", r#"{"sessionId":"pane-1"}"#),
            Some("pane-1".to_string())
        );
        assert_eq!(
            terminal_event_pane_id("terminal:exit", r#""pane-2""#),
            Some("pane-2".to_string())
        );
    }

    #[test]
    fn pairing_peer_falls_back_to_unknown_device() {
        assert_eq!(peer_description(&HeaderMap::new()), "unknown device");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)"),
        );
        assert_eq!(
            peer_description(&headers),
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)"
        );
    }

    #[test]
    fn pending_slot_budget_is_separate_and_small() {
        use super::{try_acquire_connection, try_acquire_pending_connection};
        // Pending budget is strictly smaller than the real connection budget,
        // so an unapproved peer can never pin all real slots.
        const { assert!(super::MAX_PENDING_RELAY_CONNECTIONS < super::MAX_RELAY_CONNECTIONS) };

        // Fill the pending budget; further pending acquisitions are rejected.
        // The guards must be kept alive (they release the slot on drop).
        let mut pending_guards: Vec<super::PendingRelayConnectionGuard> = Vec::new();
        for _ in 0..super::MAX_PENDING_RELAY_CONNECTIONS {
            let guard = try_acquire_pending_connection().expect("pending slot should be free");
            pending_guards.push(guard);
        }
        assert!(try_acquire_pending_connection().is_none());

        // Real slots are unaffected by the exhausted pending budget.
        let mut real_guards: Vec<super::RelayConnectionGuard> = Vec::new();
        for _ in 0..super::MAX_RELAY_CONNECTIONS {
            let guard = try_acquire_connection().expect("real slot should be free");
            real_guards.push(guard);
        }
        assert!(try_acquire_connection().is_none());

        // Dropping a real slot frees it up again.
        real_guards.pop();
        assert!(try_acquire_connection().is_some());
    }
}
