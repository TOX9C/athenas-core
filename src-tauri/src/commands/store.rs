use super::{caps, CommandError};
use crate::state::AppState;
use tauri::State;

/// Get a value from the persistent key-value store.
#[tauri::command]
pub fn store_get(state: State<'_, AppState>, key: String) -> Result<String, CommandError> {
    caps::validate_key(&key).map_err(CommandError::InvalidInput)?;
    if key == "llm.api_key" {
        // Fast path: check the confirmation flag written by store_set.
        // This avoids an expensive (and permission-dependent) keyring access
        // on every panel mount.
        if let Ok(Some(ref s)) = state.store.get::<String>("llm.api_key_status") {
            if s == "set" {
                log::debug!("[store_get] llm.api_key => confirmed by flag");
                return Ok("set".to_string());
            }
        }
        // Check keyring second
        if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
            match entry.get_password() {
                Ok(_) => {
                    log::debug!("[store_get] llm.api_key => found in OS keyring");
                    // Repair the confirmation flag in case it was lost
                    let _ = state.store.set_sync("llm.api_key_status", &"set");
                    return Ok("set".to_string());
                }
                Err(keyring::Error::NoEntry) => {
                    // Key genuinely doesn't exist; nothing to do
                }
                Err(e) => {
                    log::warn!("[store_get] llm.api_key => keyring read failed: {}", e);
                }
            }
        } else {
            log::warn!("[store_get] llm.api_key => failed to open keyring entry");
        }
        // Fallback: check store for a legacy plaintext key and migrate it
        if let Ok(Some(value)) = state.store.get::<String>(&key) {
            if !value.is_empty() && value != "not_set" && value != "set" {
                // Found a raw key in the store — migrate to keyring
                if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
                    let _ = entry.set_password(&value);
                }
                // Delete the plaintext key from the store so it never leaks again
                let _ = state.store.delete_sync(&key);
                // Write the confirmation flag too
                let _ = state.store.set_sync("llm.api_key_status", &"set");
                return Ok("set".to_string());
            }
        }
        return Ok("not_set".to_string());
    }

    state
        .store
        .get::<String>(&key)
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Key '{}' not found", key)))
}

/// Set a value in the persistent key-value store.
#[tauri::command]
pub fn store_set(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    caps::validate_key(&key)?;
    // Block writes to sensitive key namespaces from the frontend to prevent
    // key tampering and unauthorized secrets storage.
    const FORBIDDEN_PREFIXES: &[&str] = &["secret.", "auth.", "password.", "credential."];
    for prefix in FORBIDDEN_PREFIXES {
        if key.starts_with(prefix) {
            return Err(format!(
                "Writing to key namespace '{}' is forbidden",
                prefix
            ));
        }
    }
    if key == "llm.api_key" {
        if !value.is_empty() && value != "set" && value != "not_set" {
            // Store the API key securely in the OS keyring, never in plaintext
            let entry = keyring::Entry::new("athena", "api_key")
                .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
            entry
                .set_password(&value)
                .map_err(|e| format!("Failed to store API key in keyring: {}", e))?;
            log::info!(
                "[store_set] API key saved to OS keyring (service='athena', account='api_key')"
            );
            // Remove any legacy plaintext key from the store
            let _ = state.store.delete_sync(&key);
            // Write a lightweight confirmation flag so the frontend can
            // check key status without hitting the keyring (avoids keychain
            // lockout / permission-denied races on mount).
            let _ = state.store.set_sync("llm.api_key_status", &"set");
        } else if value.is_empty() || value == "not_set" {
            // Clear the API key from the keyring
            let entry = keyring::Entry::new("athena", "api_key")
                .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
            let _ = entry.delete_credential();
            log::info!(
                "[store_set] API key removed from OS keyring (service='athena', account='api_key')"
            );
            let _ = state.store.delete_sync(&key);
            let _ = state.store.delete_sync("llm.api_key_status");
        }
        return Ok(());
    }

    state
        .store
        .set_sync(&key, &value)
        .map_err(|e| e.to_string())
}

/// Check whether a key exists in the persistent key-value store.
#[tauri::command]
pub fn store_has(state: State<'_, AppState>, key: String) -> bool {
    if key == "llm.api_key" {
        if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
            if entry.get_password().is_ok() {
                return true;
            }
        }
    }
    state.store.has(&key)
}

/// Delete a key from the persistent key-value store.
#[tauri::command]
pub fn store_delete(state: State<'_, AppState>, key: String) -> Result<(), String> {
    caps::validate_key(&key)?;
    if key == "llm.api_key" {
        let entry = keyring::Entry::new("athena", "api_key")
            .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
        let _ = entry.delete_credential();
    }
    state.store.delete_sync(&key).map_err(|e| e.to_string())
}

/// Test whether the LLM API key can be read from the keyring and return a
/// structured result for the Settings UI to display.
#[tauri::command]
pub fn test_llm_api_key(state: State<'_, AppState>) -> Result<String, String> {
    // Returns a serialized JSON string ({ ok, message }) — the frontend bridge
    // casts the IPC result to a String and parses it with serde_json::from_str,
    // so this command must emit a JSON *string*, not a bare JSON object.
    let result = test_llm_api_key_value(state);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Inner implementation returning the structured JSON value. Kept separate so the
/// logic is unit-testable without going through string serialization.
fn test_llm_api_key_value(state: State<'_, AppState>) -> serde_json::Value {
    // 1. Fast path: if the confirmation flag is missing, the key was never saved
    match state.store.get::<String>("llm.api_key_status") {
        Ok(Some(ref s)) if s == "set" => { /* fall through to keyring test */ }
        _ => {
            return serde_json::json!({
                "ok": false,
                "message": "No API key configured. Save one in Settings first."
            });
        }
    }

    // 2. Try to read the keyring
    match keyring::Entry::new("athena", "api_key") {
        Ok(entry) => match entry.get_password() {
            Ok(key) => {
                if key.is_empty() {
                    serde_json::json!({
                        "ok": false,
                        "message": "API key is empty in keyring. Re-save it in Settings."
                    })
                } else {
                    serde_json::json!({
                        "ok": true,
                        "message": "API key read successfully from keyring."
                    })
                }
            }
            Err(keyring::Error::NoEntry) => serde_json::json!({
                "ok": false,
                "message": "API key not found in keyring. Save it again in Settings."
            }),
            Err(e) => {
                log::warn!("[test_llm_api_key] keyring read failed: {}", e);
                serde_json::json!({
                    "ok": false,
                    "message": format!("Keychain access failed: {}. Unlock your keychain and try again.", e)
                })
            }
        },
        Err(e) => serde_json::json!({
            "ok": false,
            "message": format!("Failed to open keyring entry: {}", e)
        }),
    }
}
