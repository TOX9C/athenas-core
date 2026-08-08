//! WebSocket upgrade handler. Holds the `RelayCtx` (an `AppHandle`) and
//! routes each incoming `invoke`/`listen`/`unlisten` frame: invoke calls go
//! to `dispatch::dispatch`; listen registers a real Tauri event listener
//! whose forwarded payload is pushed back to the phone over a writer task.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tauri::{EventId, Listener, Manager};
use tokio::sync::mpsc;

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
    ws.protocols([expected_protocol])
        .on_upgrade(move |socket| session_loop(socket, ctx))
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
async fn session_loop(socket: WebSocket, ctx: RelayCtx) {
    log::info!("[relay] ws session opened");

    let app = ctx.app_handle.clone();
    let (mut sink, mut stream) = socket.split();
    // Event callbacks run outside this async session task. An unbounded
    // sender lets them enqueue raw PTY frames without blocking/panicking in a
    // Tokio runtime; the dedicated writer remains the only socket writer.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Writer task: drains the mpsc channel and pushes frames to the sink.
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink.send(Message::Text(frame)).await.is_err() {
                break;
            }
        }
        // Close the sink cleanly so the peer sees the FIN.
        let _ = sink.send(Message::Close(None)).await;
    });

    // Registered Tauri event listeners, keyed by the shim-side listener id
    // (so `unlisten` and disconnect cleanup can find them). Guarded by a
    // mutex because Tauri's event handlers fire from arbitrary threads.
    let listeners: Arc<Mutex<HashMap<String, EventId>>> = Arc::new(Mutex::new(HashMap::new()));

    // Per-connection registry of pane/session ids the phone may observe.
    // Mobile Mirror intentionally attaches to desktop-created workspace panes,
    // not only panes spawned by the phone, so the workspace ids are included
    // alongside the connection's own spawned ids. The pairing token remains
    // the trust boundary for this read/write surface.
    let owned_pane_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let workspace_pane_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(
        workspace_pane_ids(&app),
    ));

    while let Some(frame) = stream.next().await {
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

        if text.len() > 64 * 1024 {
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
                let pane_id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string());
                // NOTE on per-pane isolation: we intentionally do NOT gate
                // pty_write/pty_resize/pty_kill to phone-spawned panes here.
                // The mobile terminal's primary flow writes into DESKTOP-created
                // panes (mobile.rs selects an existing workspace pane and sends
                // commands via pty_write). Gating writes to owned panes would
                // break that flow. Per-pane isolation is a separate design
                // decision (see audit notes) since the pull-based read RPCs
                // (output_buffer_get/get_pane_history) are the active leak
                // surface and also touch desktop panes. We track spawn/kill
                // ids only to feed the event-forwarding filter below, which is
                // defense-in-depth for a path the phone does not open today.
                let result = if dispatch::command_allowed(&cmd) {
                    dispatch::dispatch(&ctx, &cmd, args).await
                } else {
                    Err(format!(
                        "relay command not available in read-only mode: {cmd}"
                    ))
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
                let _ = tx.send(resp.to_string());
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
                let event_name = event.clone();
                let app_for_listen = app.clone();
                let owned_for_filter = Arc::clone(&owned_pane_ids);
                let workspace_for_filter = Arc::clone(&workspace_pane_ids);
                // Whether this event family carries terminal/console payloads
                // keyed by a pane id the phone must own. Non-terminal events
                // (notifications, plugin registry, fs:change, agent status)
                // are forwarded unfiltered — they are not per-pane streams.
                let is_terminal_stream = matches!(
                    event_name.as_str(),
                    "output-capture:batch" | "pty:raw" | "terminal:data" | "terminal:exit"
                );
                let eid = app_for_listen.listen_any(event_name.clone(), move |ev| {
                    let payload = ev.payload().to_string();
                    // Terminal-stream forwarding is gated on the phone owning
                    // the pane whose id the event carries. This is the real
                    // credential-leak closure: even though listen_any fires
                    // for every desktop pane, a paired phone only receives
                    // output/exit events for panes IT spawned via pty_spawn.
                    if is_terminal_stream {
                        // terminal:exit is emitted as a bare session id string
                        // (pty.rs:619); the others carry JSON objects whose
                        // pane id is at "paneId" or "sessionId".
                        let owns = if event_name == "terminal:exit" {
                            let id = payload.trim().trim_matches('"');
                            owned_for_filter.lock().contains(id)
                                || workspace_for_filter.lock().contains(id)
                        } else {
                            serde_json::from_str::<serde_json::Value>(&payload)
                                .ok()
                                .and_then(|v| {
                                    v.get("paneId")
                                        .or_else(|| v.get("sessionId"))
                                        .or_else(|| v.get("session_id"))
                                        .and_then(|id| id.as_str())
                                        .map(|s| s.to_string())
                                })
                                .map(|id| {
                                    owned_for_filter.lock().contains(&id)
                                        || workspace_for_filter.lock().contains(&id)
                                })
                                .unwrap_or(false)
                        };
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
                    let _ = tx_clone.send(out.to_string());
                });
                if let Some(previous) = listeners.lock().insert(lid, eid) {
                    // Re-registering the same shim listener id must not leave
                    // the old Tauri callback alive (reconnects can replay
                    // listener registrations).
                    app.unlisten(previous);
                }
            }
            "unlisten" => {
                let lid = msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                log::debug!("[relay] unlisten: ({lid})");
                let eid = listeners.lock().remove(&lid);
                if let Some(id) = eid {
                    app.unlisten(id);
                }
            }
            _ => {
                log::warn!("[relay] unknown ws message kind: {kind}");
            }
        }
    }

    // Cleanup: drop every registered listener so no callbacks outlive the
    // phone session, then close the writer channel so the writer task exits.
    let registered: Vec<EventId> = {
        let mut guard = listeners.lock();
        guard.drain().map(|(_, eid)| eid).collect()
    };
    for eid in registered {
        app.unlisten(eid);
    }
    drop(tx);
    let _ = writer.await;

    log::info!("[relay] ws session ended");
}

fn auth_subprotocol_matches(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').any(|item| item.trim() == expected))
        .unwrap_or(false)
}

/// Snapshot pane ids from the persisted workspace configuration. This is
/// intentionally refreshed once per WebSocket connection: workspace state is
/// the phone's attachment surface, while the pairing token is the trust
/// boundary for the relay itself.
fn workspace_pane_ids(app: &tauri::AppHandle) -> HashSet<String> {
    let state = app.state::<crate::state::AppState>();
    let raw: Option<String> = state.store.get("workspaces").ok().flatten();
    raw.and_then(|raw: String| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| value.get("spaces").cloned())
        .and_then(|spaces| spaces.as_array().cloned())
        .into_iter()
        .flatten()
        .flat_map(|space| {
            space
                .get("panes")
                .and_then(|panes| panes.as_array())
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|pane| pane.get("id").and_then(|id| id.as_str()).map(str::to_owned))
        .collect()
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
}

#[cfg(test)]
mod tests {
    use super::{auth_subprotocol_matches, event_allowed};
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
        assert!(!auth_subprotocol_matches(&HeaderMap::new(), "athena-relay.secret"));
    }

    #[test]
    fn allows_mobile_workspace_events() {
        assert!(event_allowed("output-capture:batch"));
        assert!(event_allowed("notifications:new"));
        assert!(event_allowed("fs:change:/workspace/file"));
    }

    #[test]
    fn allows_authenticated_raw_pty_streams() {
        // terminal:data remains disabled; pty:raw is the authenticated live
        // stream consumed by mobile xterm.
        assert!(!event_allowed("terminal:data"));
        assert!(event_allowed("pty:raw"));
        assert!(!event_allowed("pty:raw:abc-123"));
    }
    #[test]
    fn rejects_arbitrary_event_subscriptions() {
        assert!(!event_allowed("secret:credentials"));
        assert!(!event_allowed("tauri://menu"));
        assert!(!event_allowed(""));
    }
}
