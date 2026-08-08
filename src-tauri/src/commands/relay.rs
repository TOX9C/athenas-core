//! Tauri commands for the mobile-mirror relay. Runtime-toggled from the
//! Settings panel; the `relay.enabled` store key persists the on/off state
//! across app restarts.
//!
//! `relay_start` builds and starts the server (idempotent if already running);
//! `relay_stop` tears it down; `relay_status` reports running + LAN URL for
//! the UI to render a QR code / status pill.

use tauri::{Manager, State};
use uuid::Uuid;

use crate::relay;
use crate::state::AppState;
use parking_lot::Mutex;

static RELAY_COMMAND_LOCK: Mutex<()> = Mutex::new(());

/// Persisted store key for the relay-enabled flag.
pub const RELAY_ENABLED_KEY: &str = "relay.enabled";

fn relay_keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("athena", "relay_token")
        .map_err(|e| format!("failed to open secure relay credential: {e}"))
}

pub fn relay_token() -> Result<String, String> {
    let valid = |token: &str| token.len() >= 32 && token.chars().all(|c| c.is_ascii_hexdigit());
    let entry = relay_keyring_entry()?;
    if let Ok(token) = entry.get_password() {
        if valid(&token) {
            return Ok(token);
        }
    }
    let token = Uuid::new_v4().simple().to_string();
    entry
        .set_password(&token)
        .map_err(|e| format!("failed to persist secure relay credential: {e}"))?;
    Ok(token)
}

/// Revoke the existing pairing token so old QR/deep links cannot authenticate
/// if the user disables the mirror. The next start creates a fresh token.
pub fn revoke_relay_token() -> Result<(), String> {
    let entry = relay_keyring_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("failed to revoke secure relay credential: {error}")),
    }
}

/// Start the relay server. Idempotent — returns the bound address if already
/// running. Persists `relay.enabled = "true"` so the next boot auto-starts.
#[tauri::command]
pub async fn relay_start(state: State<'_, AppState>) -> Result<String, String> {
    let _command_lock = RELAY_COMMAND_LOCK.lock();
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
/// `relay.enabled` and revokes the pairing token so old links cannot be reused.
#[tauri::command]
pub async fn relay_stop(state: State<'_, AppState>) -> Result<(), String> {
    let _command_lock = RELAY_COMMAND_LOCK.lock();
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
    revoke_relay_token()?;
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
