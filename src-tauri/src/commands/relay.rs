//! Tauri commands for the mobile-mirror relay. Runtime-toggled from the
//! Settings panel; the `relay.enabled` key records the last UI state, but it
//! does not auto-start the plaintext LAN service on a normal public launch.
//!
//! `relay_start` builds and starts the server (idempotent if already running);
//! `relay_stop` tears it down; `relay_status` reports running + LAN URL for
//! the UI to render a QR code / status pill.

use tauri::{Emitter, Manager, State};
use uuid::Uuid;

use crate::relay;
use crate::state::AppState;
static RELAY_COMMAND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Persisted store key for the relay-enabled flag.
pub const RELAY_ENABLED_KEY: &str = "relay.enabled";

fn relay_keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("athena", "relay_token")
        .map_err(|e| format!("failed to open secure relay credential: {e}"))
}

/// Return a fresh pairing token for a new relay session.
///
/// The token is deliberately **not persisted**: every `relay_start` (and the
/// debug-only boot auto-start) mints a new one, so a QR/deep link captured in
/// a previous session cannot authenticate again. The token remains stable for
/// the lifetime of the running relay (stored in `RelayHandle`), and `relay_stop`
/// tears the whole session down. This is the "rotate token" security posture.
pub fn relay_token() -> Result<String, String> {
    Ok(Uuid::new_v4().simple().to_string())
}

/// Remove any relay pairing token persisted by an older build that used the
/// OS keychain. Kept as a best-effort cleanup so a stale token can never be
/// revived after upgrading. No-op if no entry exists.
pub fn revoke_relay_token() -> Result<(), String> {
    let entry = relay_keyring_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("failed to revoke secure relay credential: {error}")),
    }
}

/// Start the relay server. Idempotent — returns the bound address if already
/// running. Persists the UI state, but normal app startup still requires an
/// explicit per-process user action (or the development-only autostart opt-in).
#[tauri::command]
pub async fn relay_start(state: State<'_, AppState>) -> Result<String, String> {
    let _command_lock = RELAY_COMMAND_LOCK.lock().await;
    // Clone the AppHandle out of the state under a short lock.
    let app = state
        .app_handle
        .lock()
        .clone()
        .ok_or_else(|| "app handle not available".to_string())?;
    let resource_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| std::path::PathBuf::new());
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let dist_dir = relay::resolve_dist_dir(&resource_dir, &exe_dir);
    let token = relay_token()?;
    // Persist the preference before starting. If this fails, the runtime is
    // untouched and the UI receives a truthful error.
    let val = "true".to_string();
    state
        .store
        .set_sync(RELAY_ENABLED_KEY, &val)
        .map_err(|e| e.to_string())?;
    // `relay::start` builds its own tokio runtime + spawns the accept task,
    // then returns quickly. Run on spawn_blocking to avoid stalling the async
    // command runtime.
    let addr = match tokio::task::spawn_blocking(move || relay::start(app, dist_dir, token)).await {
        Ok(Ok(addr)) => addr,
        Ok(Err(e)) => {
            let _ = state
                .store
                .set_sync(RELAY_ENABLED_KEY, &"false".to_string());
            return Err(e);
        }
        Err(e) => {
            let _ = state
                .store
                .set_sync(RELAY_ENABLED_KEY, &"false".to_string());
            return Err(format!("relay start task panicked: {e}"));
        }
    };
    Ok(addr.to_string())
}

/// Stop the relay server. Idempotent — no error if already stopped. Clears
/// `relay.enabled`. The ephemeral port is released and the in-memory token
/// destroyed with the handle; the token is not persisted, so there is nothing
/// left to revoke on disk.
#[tauri::command]
pub async fn relay_stop(state: State<'_, AppState>) -> Result<(), String> {
    let _command_lock = RELAY_COMMAND_LOCK.lock().await;
    let val = "false".to_string();
    state
        .store
        .set_sync(RELAY_ENABLED_KEY, &val)
        .map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(relay::stop)
        .await
        .map_err(|e| {
            // Restore the persisted preference if the shutdown task itself
            // panics, keeping the next boot consistent with the live server.
            let _ = state.store.set_sync(RELAY_ENABLED_KEY, &"true".to_string());
            format!("relay stop task panicked: {e}")
        })?;
    // Best-effort: delete any token a pre-rotation build persisted in the
    // keychain so it cannot be reused.
    let _ = revoke_relay_token();
    Ok(())
}

/// Report the current relay status — running flag, LAN URL, and bound port —
/// for the Settings panel status pill + QR code.
///
/// This deliberately returns a JSON string because the frontend bridge's
/// existing complex-response convention is `TauriResult<String>`.
#[tauri::command]
pub fn relay_status(_state: State<'_, AppState>) -> Result<String, String> {
    serde_json::to_string(&relay::status()).map_err(|e| e.to_string())
}

/// Mark a pane as shared (or unshared) with the mobile mirror. Desktop-only —
/// this command is intentionally absent from the relay `command_allowed` list
/// so a paired phone cannot self-authorize arbitrary panes. Shared panes are
/// held in memory (default off) and reset when the app exits.
#[tauri::command]
pub fn relay_set_pane_shared(
    state: State<'_, AppState>,
    pane_id: String,
    shared: bool,
) -> Result<(), String> {
    let mut panes = state.relay_shared_panes.lock();
    if shared {
        panes.insert(pane_id);
    } else {
        panes.remove(&pane_id);
    }
    Ok(())
}

/// List the panes currently shared with the mobile mirror (sorted for a stable
/// UI order). Desktop-only, like `relay_set_pane_shared`. Returns a JSON array
/// string to match the `relay_status` string-return convention.
#[tauri::command]
pub fn relay_list_shared_panes(state: State<'_, AppState>) -> Result<String, String> {
    let mut ids: Vec<String> = state.relay_shared_panes.lock().iter().cloned().collect();
    ids.sort();
    serde_json::to_string(&ids).map_err(|e| e.to_string())
}

/// Approve or deny a pending Mobile Mirror pairing request. Called by the
/// desktop pairing-confirmation prompt; the waiting WS upgrade task resolves
/// against this decision (or its own timeout). A missing/already-resolved
/// request id is a no-op success — the upgrade may have already timed out or
/// the operator dismissed the prompt twice.
#[tauri::command]
pub fn relay_pairing_respond(
    state: State<'_, AppState>,
    request_id: String,
    approved: bool,
) -> Result<(), String> {
    if let Some(tx) = state.relay_pairing_requests.lock().remove(&request_id) {
        // The receiver may already be gone (upgrade timed out); that's fine.
        let _ = tx.send(approved);
    }
    Ok(())
}

/// A paired phone asked the desktop to share one of its panes. Emits
/// `relay:paneShareRequest` so the desktop UI can prompt the operator; the
/// operator's approval calls `relay_set_pane_shared`. This is a plain (non-
/// command) function: the phone reaches it through the relay dispatch, and the
/// desktop never invokes it over IPC — so it is intentionally NOT registered
/// in `main.rs`/`build.rs`/capabilities.
pub fn relay_request_pane_share(state: State<'_, AppState>, pane_id: String) -> Result<(), String> {
    // Dedup: if the pane is already shared, there is nothing to prompt for.
    if state.relay_shared_panes.lock().contains(&pane_id) {
        return Ok(());
    }
    // Rate-limit: at most one share prompt per pane per window, so a paired
    // phone cannot spam the desktop operator with approval dialogs.
    const SHARE_PROMPT_MIN_INTERVAL_MS: u64 = 10_000;
    let now = crate::commands::now_ms();
    {
        let mut last = state.relay_pane_share_last_request.lock();
        if let Some(&prev) = last.get(&pane_id) {
            if now.saturating_sub(prev) < SHARE_PROMPT_MIN_INTERVAL_MS {
                return Ok(());
            }
        }
        last.insert(pane_id.clone(), now);
    }
    let handle = state
        .get_app_handle()
        .ok_or_else(|| "app handle not available".to_string())?;
    let payload = serde_json::json!({ "paneId": pane_id });
    handle
        .emit("relay:paneShareRequest", payload.to_string())
        .map_err(|e| format!("failed to emit pane share request: {e}"))
}
