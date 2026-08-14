//! LAN relay + mobile-mirror server.
//!
//! Serves the desktop frontend `dist/` over HTTP and exposes a WebSocket RPC
//! bridge that replaces Tauri's `__TAURI__` IPC for phone WebViews on the same
//! network. Every `window.__TAURI__.core.invoke(cmd, args)` call from the phone
//! goes through `ws://<desktop-ip>:8787/ws`; the relay dispatches to the *real*
//! command implementations and forwards backend events back to connected
//! listeners. The phone sees one real Athena's Core instance — same state,
//! same sessions, same PTY terminals — exactly as the desktop app does.
//!
//! Lifecycle: the server is runtime-toggled from the Settings panel via the
//! `relay_start` / `relay_stop` / `relay_status` Tauri commands. When started,
//! it builds a dedicated tokio runtime, binds `0.0.0.0:8787`, and stores the
//! runtime + a shutdown signal in a process-global [`RelayState`]. Dropping the
//! [`RelayHandle`] stops the server (cancels the accept loop, drops the
//! runtime, frees the port). On app boot, persisted state is not sufficient to
//! auto-start this experimental plaintext service; `main.rs` requires the
//! explicit debug-build-only `ATHENA_RELAY_AUTOSTART=1` opt-in; release
//! binaries compile relay autostart out.

mod discovery;
mod dispatch;
mod shim;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use parking_lot::Mutex;
use tauri::AppHandle;
use tokio::sync::oneshot;
use tower_http::services::ServeDir;

/// Default listen port for the relay. Bound to `0.0.0.0` so mobile devices on
/// the same LAN can reach it.
pub const RELAY_PORT: u16 = 8787;

/// Shared relay context. Cheap to clone — `AppHandle` is an `Arc` internally.
/// Each command dispatch borrows `State<'_, AppState>` from the handle for the
/// duration of that one call, matching the pattern at `main.rs:298`.
#[derive(Clone)]
pub struct RelayCtx {
    pub app_handle: AppHandle,
    pub dist_dir: String,
    /// Per-process capability token required for WebSocket RPC access.
    /// The token is included in the QR/deep link shown by the desktop UI.
    pub token: String,
    /// Bound address used by the discovery descriptor.
    pub addr: SocketAddr,
}

/// Handle to a running relay server. Dropping it stops the server: the
/// shutdown signal fires, the accept loop exits, the driver thread ends, and
/// the port is released. Stored in the global [`RELAY_STATE`].
struct RelayHandle {
    /// Signal fired on drop to tell the accept loop to stop. `Option` so
    /// `Drop::drop` can `take()` it early — closing the channel *before*
    /// joining the driver. Without the take the sender would only drop
    /// *after* the drop body returns, but the body blocks on join which
    /// waits on shutdown_rx which waits on the sender dropping: deadlock.
    _shutdown: Option<oneshot::Sender<()>>,
    /// Driver thread join handle. Joined when the handle drops so the thread
    /// exits before the runtime is dropped (avoiding a panic-on-drop race).
    /// `Option` so `Drop` can `take()` it (a `JoinHandle` can't be moved out
    /// of `&mut self`).
    driver: Option<std::thread::JoinHandle<()>>,
    /// The dedicated runtime. Never accessed after construction — its job
    /// is to stay alive in this field so that dropping `RelayHandle` drops
    /// the runtime, which cancels every spawned task and closes the listener.
    #[allow(dead_code)]
    runtime: tokio::runtime::Runtime,
    /// The bound address (for status reporting).
    addr: SocketAddr,
    /// Secret required by mobile clients during the WebSocket handshake.
    token: String,
    /// mDNS daemon kept alive for the lifetime of the relay.
    #[allow(dead_code)]
    discovery: Option<mdns_sd::ServiceDaemon>,
}

impl Drop for RelayHandle {
    fn drop(&mut self) {
        // Fire the shutdown signal FIRST. Taking the sender out of the
        // Option and dropping it closes the oneshot channel; the driver's
        // `shutdown_rx` arm wins its `select!`, `axum::serve` returns, and
        // the driver thread exits. Doing this *before* join is essential —
        // Rust drops fields in declaration order only AFTER the body returns,
        // so the body must explicitly fire the signal, or join blocks forever.
        if let Some(tx) = self._shutdown.take() {
            // send() closes the receiver on Send or on the sender dropping;
            // either way the driver's shutdown_rx arm wins its select!.
            let _ = tx.send(());
            log::info!("[relay] shutdown signal sent");
        }
        // Wait for the driver thread to finish so we don't drop the runtime
        // out from under a still-running accept loop. Now safe: shutdown fired.
        if let Some(driver) = self.driver.take() {
            let _ = driver.join();
        }
        // Dropping `runtime` cancels all its tasks and frees the port.
        log::info!(
            "[relay] runtime dropped, port {} released",
            self.addr.port()
        );
    }
}

/// Process-global relay state. `Some` while the server is running; `None` when
/// stopped. Guarded by a mutex because `relay_start`/`relay_stop`/`relay_status`
/// are called from Tauri command threads.
static RELAY_STATE: Mutex<Option<Arc<RelayHandle>>> = Mutex::new(None);
/// Serializes start/stop so concurrent commands cannot race between the
/// state check and publishing/removing the running handle.
static RELAY_LIFECYCLE: Mutex<()> = Mutex::new(());

/// Resolve the frontend `dist/` directory by probing a few candidate paths
/// and returning the first whose `index.html` exists. Works regardless of the
/// launch CWD (dev `cargo run`, bundled app, etc.).
pub fn resolve_dist_dir(resource: &std::path::Path, exe_dir: &std::path::Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let candidates = [
        resource.join("frontend").join("dist"),
        // packaged Tauri resources map frontend/dist files directly under the
        // resource root (see tauri.conf.json).
        resource.to_path_buf(),
        resource.join("dist"),
        // dev: <workspace>/frontend/dist (CWD is the workspace root)
        cwd.join("frontend").join("dist"),
        // dev: <workspace>/src-tauri/../frontend/dist
        cwd.join("..").join("frontend").join("dist"),
        // dev via cargo run from src-tauri: exe is target/debug
        exe_dir
            .join("..")
            .join("..")
            .join("..")
            .join("frontend")
            .join("dist"),
    ];
    candidates
        .iter()
        .find(|p| p.join("index.html").exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| resource.to_string_lossy().to_string())
}

/// Start the relay HTTP+WS server on a dedicated runtime. Returns the bound
/// address. If a relay is already running, returns its address without
/// restarting.
///
/// Panics if the port can't be bound (another process owns it). The caller
/// (the `relay_start` command) catches bind failures and reports them to the
/// frontend as an error string.
pub fn start(app: AppHandle, dist_dir: String, token: String) -> Result<SocketAddr, String> {
    let _lifecycle = RELAY_LIFECYCLE.lock();
    // If already running, return the existing address (idempotent start).
    if let Some(handle) = RELAY_STATE.lock().as_ref() {
        return Ok(handle.addr);
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], RELAY_PORT));
    let ctx = RelayCtx {
        app_handle: app.clone(),
        dist_dir: dist_dir.clone(),
        token: token.clone(),
        addr,
    };

    let router = Router::new()
        .route("/__relay_shim__.js", get(shim::serve_shim))
        .route("/__athena_discovery__.json", get(shim::serve_discovery))
        .route("/", get(shim::serve_index))
        .route("/index.html", get(shim::serve_index))
        .route("/mobile.html", get(shim::serve_mobile))
        .route("/ws", get(ws::handle_upgrade))
        .fallback_service(ServeDir::new(dist_dir))
        .with_state(ctx);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("athena-relay")
        .build()
        .map_err(|e| format!("failed to build relay runtime: {e}"))?;

    let bound = runtime
        .block_on(tokio::net::TcpListener::bind(addr))
        .map_err(|e| format!("relay bind failed on {addr}: {e}"))?;
    let bound_addr = bound.local_addr().unwrap_or(addr);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let runtime_handle = runtime.handle().clone();

    let driver = std::thread::Builder::new()
        .name("athena-relay-driver".into())
        .spawn(move || {
            runtime_handle.block_on(async {
                let serve = axum::serve(bound, router);
                tokio::select! {
                    res = serve => {
                        if let Err(e) = res {
                            log::error!("[relay] server error: {e}");
                        }
                    }
                    _ = shutdown_rx => {
                        log::info!("[relay] graceful shutdown received");
                    }
                };
            });
        })
        .map_err(|e| format!("failed to spawn relay driver: {e}"))?;

    let handle = Arc::new(RelayHandle {
        _shutdown: Some(shutdown_tx),
        driver: Some(driver),
        runtime,
        addr: bound_addr,
        token,
        discovery: discovery::advertise(bound_addr),
    });
    *RELAY_STATE.lock() = Some(handle);

    log::info!("[relay] listening on http://{bound_addr} — authenticated mobile mirror active");
    print_relay_url(bound_addr);

    Ok(bound_addr)
}

/// Stop the relay server if running. No-op if already stopped. Drops the
/// [`RelayHandle`] which fires the shutdown signal, joins the driver thread,
/// and releases the port.
pub fn stop() {
    let _lifecycle = RELAY_LIFECYCLE.lock();
    let taken = RELAY_STATE.lock().take();
    if let Some(handle) = taken {
        // Dropping the Arc<RelayHandle> — if this is the last ref, the handle
        // drops and the server shuts down. We hold the only ref besides
        // possibly a concurrent status query, which clones its own Arc.
        drop(handle);
        log::info!("[relay] stopped");
    }
}

/// Status snapshot for the `relay_status` command.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelayStatus {
    pub running: bool,
    pub url: Option<String>,
    pub port: u16,
    /// Base64-encoded SVG QR code for the current LAN URL. This stays in the
    /// local Tauri response and is never served by the relay itself.
    pub qr_svg_base64: Option<String>,
}

/// Build the LAN URL from an already-held relay handle.
fn lan_url_for(handle: &RelayHandle) -> String {
    let host = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    format!(
        "http://{host}:{}/mobile.html?mobile=1#token={}",
        handle.addr.port(),
        handle.token
    )
}

/// Render a QR code without exposing the token through another network
/// endpoint. The Settings UI receives the SVG only through the local Tauri
/// command response and embeds it as a data URL.
fn qr_svg_base64(url: &str) -> Option<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use qrcode::render::svg;

    qrcode::QrCode::new(url.as_bytes())
        .ok()
        .map(|code| code.render::<svg::Color>().min_dimensions(220, 220).build())
        .map(|svg| STANDARD.encode(svg.as_bytes()))
}

/// Current status — running flag + LAN URL. Cheap; safe to call from a command.
pub fn status() -> RelayStatus {
    // Do not call `lan_url()` while holding this guard: parking_lot::Mutex is
    // non-reentrant, so the old implementation deadlocked every status call
    // while the relay was running.
    let guard = RELAY_STATE.lock();
    match guard.as_ref() {
        Some(handle) => {
            let url = lan_url_for(handle);
            RelayStatus {
                running: true,
                qr_svg_base64: qr_svg_base64(&url),
                url: Some(url),
                port: handle.addr.port(),
            }
        }
        None => RelayStatus {
            running: false,
            url: None,
            port: RELAY_PORT,
            qr_svg_base64: None,
        },
    }
}

fn print_relay_url(addr: SocketAddr) {
    match local_ip_address::local_ip() {
        Ok(lan_ip) => {
            let guard = RELAY_STATE.lock();
            let token = guard
                .as_ref()
                .map(|handle| handle.token.as_str())
                .unwrap_or_default();
            let url = format!(
                "http://{lan_ip}:{port}/mobile.html?mobile=1#token={token}",
                port = addr.port()
            );
            println!("\n  ╔════════════════════════════════════════════════╗");
            println!("  ║  Athena's Core — mobile mirror                ║");
            println!("  ║  Open on your phone (same Wi-Fi):              ║");
            println!("  ║  {url:<42} ║", url = url);
            println!("  ╚════════════════════════════════════════════════╝");

            if let Ok(code) = qrcode::QrCode::new(url.as_bytes()) {
                use qrcode::render::unicode;
                let string = code
                    .render::<unicode::Dense1x2>()
                    .min_dimensions(26, 6)
                    .build();
                println!("\n{string}\n");
            } else {
                println!("[relay] (failed to generate QR code)\n");
            }
        }
        Err(e) => {
            log::warn!("[relay] could not detect LAN IP for QR code: {e}");
            println!(
                "\n  [relay] server running on 0.0.0.0:{} — set your phone's browser to this host's LAN IP\n",
                addr.port()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::qr_svg_base64;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    #[test]
    fn qr_payload_is_a_decodable_svg_for_pairing_url() {
        let encoded =
            qr_svg_base64("http://127.0.0.1:8787/mobile.html?mobile=1#token=test").unwrap();
        let svg = String::from_utf8(STANDARD.decode(encoded).unwrap()).unwrap();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
        assert!(svg.contains("<path") || svg.contains("<rect"));
    }
}
