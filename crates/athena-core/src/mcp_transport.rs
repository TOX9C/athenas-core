//! TCP transport and per-connection handling for the MCP server.

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::{timeout, Duration};

use super::mcp_dispatch;
use super::{
    AgentCommsHandler, JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpServer, OutputHandler,
    SpawnHandler, TaskHandler,
};

/// How long a connected MCP client may sit idle between requests before
/// the server disconnects it. Guards against half-open connections and
/// buggy clients that never send a newline.
const MCP_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Maximum size in bytes of a single MCP JSON-RPC request line. Caps memory
/// usage when a client streams a huge line without a newline (a cheap local
/// DoS). 1 MiB is generous for these payloads; the request-body cap in the
/// Tauri command layer (`MAX_REQUEST_BYTES`) is the same.
const MAX_MCP_LINE_BYTES: usize = 1024 * 1024;
/// Maximum number of simultaneous TCP clients. MCP is loopback-only, but a
/// local process must not be able to exhaust one Tokio task per connection.
pub(super) const MCP_MAX_CONNECTIONS: usize = 16;
/// Request-rate budget per authenticated or unauthenticated connection.
const MCP_REQUEST_WINDOW: Duration = Duration::from_secs(60);
const MCP_MAX_REQUESTS_PER_WINDOW: u32 = 240;
/// Absolute lifetime request budget for one connection. Reconnects are cheap
/// for legitimate clients, while this prevents a connection that stays alive
/// forever from accumulating unbounded work.
const MCP_MAX_REQUESTS_PER_CONNECTION: u32 = 10_000;

#[derive(Debug)]
struct RequestBudget {
    window_started: Instant,
    window_requests: u32,
    total_requests: u32,
}

impl RequestBudget {
    fn new() -> Self {
        Self {
            window_started: Instant::now(),
            window_requests: 0,
            total_requests: 0,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_started) >= MCP_REQUEST_WINDOW {
            self.window_started = now;
            self.window_requests = 0;
        }
        if self.total_requests >= MCP_MAX_REQUESTS_PER_CONNECTION
            || self.window_requests >= MCP_MAX_REQUESTS_PER_WINDOW
        {
            return false;
        }
        self.total_requests += 1;
        self.window_requests += 1;
        true
    }
}

// Transport dependencies stay explicit so the accept loop can construct each
// connection handler without hiding its security and lifecycle inputs.
#[allow(clippy::too_many_arguments)]
pub(super) async fn accept_loop(
    listener: tokio::net::TcpListener,
    shutdown: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    app_shutdown: Arc<AtomicBool>,
    token: String,
    active_clients: Arc<Mutex<HashMap<String, TcpStream>>>,
    task_handler: Option<TaskHandler>,
    spawn_handler: Option<SpawnHandler>,
    output_handler: Option<OutputHandler>,
    agent_comms_handler: Option<AgentCommsHandler>,
    tool_executor: Option<Arc<parking_lot::Mutex<super::ToolExecutor>>>,
) {
    let _stopped_guard = StoppedGuard(Arc::clone(&stopped));
    let connection_slots = Arc::new(tokio::sync::Semaphore::new(MCP_MAX_CONNECTIONS));
    log::info!(
        "MCP accept loop started (max_connections={}, max_requests_per_window={})",
        MCP_MAX_CONNECTIONS,
        MCP_MAX_REQUESTS_PER_WINDOW
    );
    loop {
        if shutdown.load(Ordering::Relaxed) || app_shutdown.load(Ordering::Relaxed) {
            log::info!("MCP accept loop stopping");
            break;
        }

        // A short timeout lets shutdown be observed even while no client is
        // connecting. The listener is dropped when this function returns,
        // releasing the bound port for a subsequent init.
        match tokio::time::timeout(Duration::from_millis(100), listener.accept()).await {
            Ok(Ok((stream, _addr))) => {
                let connection_permit = match Arc::clone(&connection_slots).try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        log::warn!("MCP: rejecting connection — connection limit reached");
                        drop(stream);
                        continue;
                    }
                };
                let handler = ConnectionHandler {
                    _connection_permit: connection_permit,
                    token: token.clone(),
                    active_clients: Arc::clone(&active_clients),
                    shutdown: Arc::clone(&shutdown),
                    app_shutdown: Arc::clone(&app_shutdown),
                    task_handler: task_handler.clone(),
                    spawn_handler: spawn_handler.clone(),
                    output_handler: output_handler.clone(),
                    agent_comms_handler: agent_comms_handler.clone(),
                    tool_executor: tool_executor.clone(),
                    authenticated: AtomicBool::new(false),
                };
                tokio::spawn(async move {
                    handler.handle_connection(stream).await;
                });
            }
            Ok(Err(e)) => {
                // A listener error during shutdown is expected; otherwise
                // retain the existing behavior and continue accepting.
                if shutdown.load(Ordering::Relaxed) || app_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                log::error!("MCP: failed to accept connection: {}", e);
            }
            Err(_) => {
                // Timeout is only used to poll the shutdown flag.
            }
        }
    }
    stopped.store(true, Ordering::Release);
}

struct StoppedGuard(Arc<AtomicBool>);

impl Drop for StoppedGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct ConnectionHandler {
    token: String,
    active_clients: Arc<Mutex<HashMap<String, TcpStream>>>,
    shutdown: Arc<AtomicBool>,
    app_shutdown: Arc<AtomicBool>,
    task_handler: Option<TaskHandler>,
    spawn_handler: Option<SpawnHandler>,
    output_handler: Option<OutputHandler>,
    agent_comms_handler: Option<AgentCommsHandler>,
    tool_executor: Option<Arc<parking_lot::Mutex<super::ToolExecutor>>>,
    authenticated: AtomicBool,
    // Held for the entire connection lifetime; dropping the handler releases
    // the slot even when setup or I/O fails before authentication.
    _connection_permit: tokio::sync::OwnedSemaphorePermit,
}

impl ConnectionHandler {
    async fn handle_request(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        super::handle_request_with_executor(
            &self.token,
            req,
            &self.task_handler,
            &self.spawn_handler,
            &self.output_handler,
            &self.agent_comms_handler,
            self.tool_executor.as_ref(),
        )
        .await
    }

    async fn handle_connection(&self, stream: tokio::net::TcpStream) {
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let connection_id = format!("{}#{}", peer, uuid::Uuid::new_v4());
        log::info!("MCP: new connection {} from {}", connection_id, peer);

        // Convert back to std briefly to get a clone for active_clients.
        let std_stream = match stream.into_std() {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to convert tokio stream to std: {}", e);
                return;
            }
        };
        let std_clone = match std_stream.try_clone() {
            Ok(c) => c,
            Err(e) => {
                log::error!("failed to clone std stream: {}", e);
                return;
            }
        };
        // Re-convert to tokio for async I/O.
        let stream = match tokio::net::TcpStream::from_std(std_stream) {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to convert std stream back to tokio: {}", e);
                return;
            }
        };

        // Split into read/write halves for non-blocking I/O.
        let (read_half, write_half) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);
        let write_half = Arc::new(tokio::sync::Mutex::new(write_half));

        // Keep the clone local until the client authenticates. Registering an
        // unauthenticated socket in active_clients would allow it to receive
        // broadcast notifications before completing `initialize`.
        let mut broadcast_stream = Some(std_clone);
        let mut broadcast_registered = false;

        // Capped line buffer: bound each request at MAX_MCP_LINE_BYTES so a
        // client streaming a never-terminated line cannot force unbounded
        // allocation before the JSON is parsed.
        let mut buf: Vec<u8> = Vec::with_capacity(8192);
        let mut request_budget = RequestBudget::new();

        loop {
            if self.shutdown.load(Ordering::Relaxed) || self.app_shutdown.load(Ordering::Relaxed) {
                break;
            }
            buf.clear();
            // Read a full line with an idle timeout, aborting if the
            // accumulated size exceeds the cap.
            let line: String = {
                let mut total: usize = 0;
                let read_result = timeout(MCP_IDLE_TIMEOUT, async {
                    loop {
                        if self.shutdown.load(Ordering::Relaxed) || self.app_shutdown.load(Ordering::Relaxed) {
                            return Ok(None);
                        }
                        let read_until = reader.read_until(b'\n', &mut buf);
                        let n = tokio::select! {
                            _ = wait_for_shutdown(&self.shutdown, &self.app_shutdown) => return Ok(None),
                            result = read_until => result,
                        };
                        let n = match n {
                            Ok(n) => n,
                            Err(e) => {
                                log::warn!("MCP: read error from {}: {}", peer, e);
                                return Err(());
                            }
                        };
                        if n == 0 {
                            return Ok(None); // EOF
                        }
                        total += n;
                        if total > MAX_MCP_LINE_BYTES {
                            log::warn!(
                                "MCP: disconnecting {} — line exceeded {} bytes",
                                peer,
                                MAX_MCP_LINE_BYTES
                            );
                            return Err(());
                        }
                        if buf.last() == Some(&b'\n') {
                            return Ok(Some(String::from_utf8_lossy(&buf).to_string()));
                        }
                    }
                })
                .await;
                match read_result {
                    Ok(Ok(Some(l))) => l,
                    Ok(Ok(None)) => break, // EOF
                    Ok(Err(())) => break,  // read error or oversize
                    Err(_) => {
                        log::info!(
                            "MCP: client {} idle for >{}s, closing connection",
                            peer,
                            MCP_IDLE_TIMEOUT.as_secs()
                        );
                        break;
                    }
                }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !request_budget.allow(Instant::now()) {
                log::warn!("MCP: disconnecting {} — request budget exceeded", peer);
                break;
            }

            let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("MCP: parse error from {}: {}", peer, e);
                    // JSON-RPC 2.0 spec: invalid JSON must yield a Parse error
                    // response. Silent drop would hang the client waiting for
                    // its reply. Best-effort recover the `id` from the
                    // partial payload; fall back to null.
                    let err = mcp_dispatch::make_parse_error_response(trimmed);
                    let mut writer = write_half.lock().await;
                    if writer.write_all((err + "\n").as_bytes()).await.is_err() {
                        break;
                    }
                    continue;
                }
            };

            // Per-connection auth gate: every non-initialize method requires
            // a successful initialize first.
            let response =
                if !self.authenticated.load(Ordering::SeqCst) && req.method != "initialize" {
                    JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: req.id.clone(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32600,
                            message: "Unauthenticated: initialize required".into(),
                            data: None,
                        }),
                    }
                } else {
                    self.handle_request(&req).await
                };

            // Only send a response for requests (notifications have no id)
            if response.id.is_some() {
                let json = McpServer::serialize_response(&response) + "\n";
                let mut writer = write_half.lock().await;
                if writer.write_all(json.as_bytes()).await.is_err() {
                    break;
                }
            }

            // On successful initialize, log it and mark connection as authenticated.
            if req.method == "initialize" && response.error.is_none() {
                self.authenticated.store(true, Ordering::SeqCst);
                if let Some(stream) = broadcast_stream.take() {
                    if let Ok(mut clients) = self.active_clients.lock() {
                        clients.insert(connection_id.clone(), stream);
                        broadcast_registered = true;
                    }
                }
                log::info!("MCP: client {} initialized", peer);
            }

            // On failed initialize, close the connection
            if req.method == "initialize" && response.error.is_some() {
                log::warn!("MCP: rejecting unauthorized client {}", peer);
                break;
            }
        }

        // Remove from active_clients on disconnect
        if broadcast_registered {
            if let Ok(mut clients) = self.active_clients.lock() {
                clients.remove(&connection_id);
            }
        }
        log::info!("MCP: connection closed from {}", peer);
    }
}

async fn wait_for_shutdown(generation_shutdown: &AtomicBool, app_shutdown: &AtomicBool) {
    while !generation_shutdown.load(Ordering::Relaxed) && !app_shutdown.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestBudget, MCP_MAX_REQUESTS_PER_WINDOW};
    use std::time::{Duration, Instant};

    #[test]
    fn request_budget_rejects_burst_after_window_limit() {
        let start = Instant::now();
        let mut budget = RequestBudget {
            window_started: start,
            window_requests: 0,
            total_requests: 0,
        };
        for _ in 0..MCP_MAX_REQUESTS_PER_WINDOW {
            assert!(budget.allow(start));
        }
        assert!(!budget.allow(start));
    }

    #[test]
    fn request_budget_resets_window_but_keeps_lifetime_count() {
        let start = Instant::now();
        let mut budget = RequestBudget {
            window_started: start,
            window_requests: MCP_MAX_REQUESTS_PER_WINDOW,
            total_requests: MCP_MAX_REQUESTS_PER_WINDOW,
        };
        assert!(budget.allow(start + Duration::from_secs(61)));
        assert_eq!(budget.window_requests, 1);
        assert_eq!(budget.total_requests, MCP_MAX_REQUESTS_PER_WINDOW + 1);
    }

    #[test]
    fn request_budget_rejects_when_lifetime_limit_is_reached() {
        let start = Instant::now();
        let mut budget = RequestBudget {
            window_started: start,
            window_requests: 0,
            total_requests: super::MCP_MAX_REQUESTS_PER_CONNECTION,
        };
        assert!(!budget.allow(start));
    }
}
