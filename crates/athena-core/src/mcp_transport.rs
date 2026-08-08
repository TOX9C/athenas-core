//! TCP transport and per-connection handling for the MCP server.

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
) {
    let _stopped_guard = StoppedGuard(Arc::clone(&stopped));
    log::info!("MCP accept loop started");
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
                let handler = ConnectionHandler {
                    token: token.clone(),
                    active_clients: Arc::clone(&active_clients),
                    shutdown: Arc::clone(&shutdown),
                    app_shutdown: Arc::clone(&app_shutdown),
                    task_handler: task_handler.clone(),
                    spawn_handler: spawn_handler.clone(),
                    output_handler: output_handler.clone(),
                    agent_comms_handler: agent_comms_handler.clone(),
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
    authenticated: AtomicBool,
}

impl ConnectionHandler {
    async fn handle_request(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        mcp_dispatch::handle_request_impl(
            &self.token,
            req,
            &self.task_handler,
            &self.spawn_handler,
            &self.output_handler,
            &self.agent_comms_handler,
        )
        .await
    }

    async fn handle_connection(&self, stream: tokio::net::TcpStream) {
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        log::info!("MCP: new connection from {}", peer);

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

        // Register the std TcpStream clone for broadcast_notification
        // (which uses sync I/O). Will be removed on disconnect.
        if let Ok(mut clients) = self.active_clients.lock() {
            clients.insert(peer.clone(), std_clone);
        }

        // Capped line buffer: bound each request at MAX_MCP_LINE_BYTES so a
        // client streaming a never-terminated line cannot force unbounded
        // allocation before the JSON is parsed.
        let mut buf: Vec<u8> = Vec::with_capacity(8192);

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
                log::info!("MCP: client {} initialized", peer);
            }

            // On failed initialize, close the connection
            if req.method == "initialize" && response.error.is_some() {
                log::warn!("MCP: rejecting unauthorized client {}", peer);
                break;
            }
        }

        // Remove from active_clients on disconnect
        if let Ok(mut clients) = self.active_clients.lock() {
            clients.remove(&peer);
        }
        log::info!("MCP: connection closed from {}", peer);
    }
}

async fn wait_for_shutdown(generation_shutdown: &AtomicBool, app_shutdown: &AtomicBool) {
    while !generation_shutdown.load(Ordering::Relaxed) && !app_shutdown.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
