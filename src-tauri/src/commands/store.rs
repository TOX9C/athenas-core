use super::{caps, CommandError};
use crate::state::AppState;
use tauri::State;

/// Map an API-key store key to its keyring account + status-flag store key.
///
/// `llm.api_key` is the legacy/custom slot → keyring account `api_key`.
/// `llm.api_key.<provider>` is a provider-scoped slot (e.g. NVIDIA NIM) →
/// keyring account `api_key_<provider>`. Returns `None` for unrelated keys.
pub(super) fn api_key_target(key: &str) -> Option<(String, String)> {
    if key == "llm.api_key" {
        return Some(("api_key".to_string(), "llm.api_key_status".to_string()));
    }
    if let Some(provider) = key.strip_prefix("llm.api_key.") {
        if !provider.is_empty()
            && provider
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Some((
                format!("api_key_{provider}"),
                format!("llm.api_key_status.{provider}"),
            ));
        }
    }
    None
}

/// Get a value from the persistent key-value store.
#[tauri::command]
pub fn store_get(state: State<'_, AppState>, key: String) -> Result<String, CommandError> {
    caps::validate_key(&key).map_err(CommandError::InvalidInput)?;
    if let Some((account, status_key)) = api_key_target(&key) {
        // Fast path: check the confirmation flag written by store_set.
        // This avoids an expensive (and permission-dependent) keyring access
        // on every panel mount.
        if let Ok(Some(ref s)) = state.store.get::<String>(&status_key) {
            if s == "set" {
                log::debug!("[store_get] {key} => confirmed by flag");
                return Ok("set".to_string());
            }
        }
        // Check keyring second
        if let Ok(entry) = keyring::Entry::new("athena", &account) {
            match entry.get_password() {
                Ok(_) => {
                    log::debug!("[store_get] {key} => found in OS keyring");
                    // Repair the confirmation flag in case it was lost
                    let _ = state.store.set_sync(&status_key, &"set");
                    return Ok("set".to_string());
                }
                Err(keyring::Error::NoEntry) => {
                    // Key genuinely doesn't exist; nothing to do
                }
                Err(e) => {
                    log::warn!("[store_get] {key} => keyring read failed: {}", e);
                }
            }
        } else {
            log::warn!("[store_get] {key} => failed to open keyring entry");
        }
        // Fallback: check store for a legacy plaintext key and migrate it
        if let Ok(Some(value)) = state.store.get::<String>(&key) {
            if !value.is_empty() && value != "not_set" && value != "set" {
                // Found a raw key in the store — migrate to keyring
                if let Ok(entry) = keyring::Entry::new("athena", &account) {
                    let _ = entry.set_password(&value);
                }
                // Delete the plaintext key from the store so it never leaks again
                let _ = state.store.delete_sync(&key);
                // Write the confirmation flag too
                let _ = state.store.set_sync(&status_key, &"set");
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
    if let Some((account, status_key)) = api_key_target(&key) {
        if !value.is_empty() && value != "set" && value != "not_set" {
            // Store the API key securely in the OS keyring, never in plaintext
            let entry = keyring::Entry::new("athena", &account)
                .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
            entry
                .set_password(&value)
                .map_err(|e| format!("Failed to store API key in keyring: {}", e))?;
            log::info!(
                "[store_set] API key saved to OS keyring (service='athena', account='{account}')"
            );
            // Remove any legacy plaintext key from the store
            let _ = state.store.delete_sync(&key);
            // Write a lightweight confirmation flag so the frontend can
            // check key status without hitting the keyring (avoids keychain
            // lockout / permission-denied races on mount).
            let _ = state.store.set_sync(&status_key, &"set");
        } else if value.is_empty() || value == "not_set" {
            // Clear the API key from the keyring
            let entry = keyring::Entry::new("athena", &account)
                .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
            let _ = entry.delete_credential();
            log::info!(
                "[store_set] API key removed from OS keyring (service='athena', account='{account}')"
            );
            let _ = state.store.delete_sync(&key);
            let _ = state.store.delete_sync(&status_key);
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
    if let Some((account, _)) = api_key_target(&key) {
        if let Ok(entry) = keyring::Entry::new("athena", &account) {
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
    if let Some((account, status_key)) = api_key_target(&key) {
        let entry = keyring::Entry::new("athena", &account)
            .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
        let _ = entry.delete_credential();
        let _ = state.store.delete_sync(&status_key);
    }
    state.store.delete_sync(&key).map_err(|e| e.to_string())
}

/// Resolve which keyring account + status flag the chat backend will use,
/// so the Settings "Test Key" check mirrors production routing.
///
/// When `llm.provider` is persisted to a preset id the provider-scoped slot
/// wins; otherwise (custom / nothing saved) the legacy slot is used. Pre-
/// scoping installs persisted `llm.provider` but wrote their key to the legacy
/// account — if the scoped slot is unset and the legacy one is set, the test
/// follows the legacy slot so it stays consistent with chat.
fn resolve_key_slot(store: &athena_store::KeyValueStore) -> (String, String) {
    let persisted = store
        .get::<String>("llm.provider")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());
    if let Some(provider) = persisted {
        // `custom` never persists `llm.provider` (it deletes it) — but guard
        // here anyway so a stale value still routes to the legacy slot.
        if provider.trim() != "custom" {
            let scoped = format!("llm.api_key.{provider}");
            if let Some(slot) = api_key_target(&scoped) {
                let scoped_set = matches!(store.get::<String>(&slot.1), Ok(Some(ref s)) if s == "set");
                if scoped_set {
                    return slot;
                }
                let legacy_set = matches!(
                    store.get::<String>("llm.api_key_status"),
                    Ok(Some(ref s)) if s == "set"
                );
                if legacy_set {
                    return ("api_key".to_string(), "llm.api_key_status".to_string());
                }
                return slot;
            }
        }
    }
    ("api_key".to_string(), "llm.api_key_status".to_string())
}

/// Test whether the LLM API key can be read from the keyring and return a
/// structured result for the Settings UI to display.
#[tauri::command]
pub fn test_llm_api_key(state: State<'_, AppState>) -> Result<String, String> {
    // Returns a serialized JSON string ({ ok, message }) — the frontend bridge
    // casts the IPC result to a String and parses it with serde_json::from_str,
    // so this command must emit a JSON *string*, not a bare JSON object.
    let (account, status_key) = resolve_key_slot(&state.store);
    let result = test_llm_api_key_value(state, &account, &status_key);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Inner implementation returning the structured JSON value. Kept separate so the
/// logic is unit-testable without going through string serialization.
fn test_llm_api_key_value(
    state: State<'_, AppState>,
    account: &str,
    status_key: &str,
) -> serde_json::Value {
    // 1. Fast path: if the confirmation flag is missing, the key was never saved
    match state.store.get::<String>(status_key) {
        Ok(Some(ref s)) if s == "set" => { /* fall through to keyring test */ }
        _ => {
            return serde_json::json!({
                "ok": false,
                "message": "No API key configured for this provider. Save one in Settings first."
            });
        }
    }

    // 2. Try to read the keyring
    match keyring::Entry::new("athena", account) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Keyring-slot mapping: the legacy `llm.api_key` maps to the bare
    /// `api_key` account, provider-scoped keys to their own accounts.
    #[test]
    fn api_key_target_maps_legacy_and_scoped_slots() {
        assert_eq!(
            api_key_target("llm.api_key"),
            Some(("api_key".to_string(), "llm.api_key_status".to_string()))
        );
        for provider in ["openai", "anthropic", "nvidia_nim", "lmstudio"] {
            assert_eq!(
                api_key_target(&format!("llm.api_key.{provider}")),
                Some((
                    format!("api_key_{provider}"),
                    format!("llm.api_key_status.{provider}"),
                )),
                "provider-scoped slot for '{provider}'"
            );
        }
    }

    /// Providers with invalid characters (or an empty provider segment) must
    /// not map to a keyring account — prevents key-name injection.
    #[test]
    fn api_key_target_rejects_invalid_provider_segments() {
        for key in [
            "llm.api_key.",              // empty provider
            "llm.api_key.a b",           // whitespace
            "llm.api_key.a.b",           // dot not allowed
            "llm.api_key/nim",           // slash not allowed
            "llm.api_key.ƒoo",           // non-ascii
            "llm.api_key_status.openai", // unrelated key
            "llm.model",                 // unrelated key
        ] {
            assert_eq!(api_key_target(key), None, "key '{key}' must be rejected");
        }
    }

    /// With no persisted `llm.provider`, the legacy slot is authoritative
    /// (custom provider / nothing saved yet).
    #[test]
    fn resolve_key_slot_defaults_to_legacy_without_provider() {
        let store = athena_store::KeyValueStore::new_empty();
        assert_eq!(
            resolve_key_slot(&store),
            ("api_key".to_string(), "llm.api_key_status".to_string())
        );
    }

    /// A persisted preset id routes to its scoped slot — even when neither
    /// the scoped nor the legacy status flag is set yet.
    #[test]
    fn resolve_key_slot_uses_scoped_slot_for_persisted_provider() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.provider", &"nvidia_nim").unwrap();
        assert_eq!(
            resolve_key_slot(&store),
            (
                "api_key_nvidia_nim".to_string(),
                "llm.api_key_status.nvidia_nim".to_string(),
            )
        );
    }

    /// A persisted preset id whose scoped flag is set wins over any legacy
    /// key that might still be around.
    #[test]
    fn resolve_key_slot_prefers_scoped_flag() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.provider", &"openai").unwrap();
        store.set_sync("llm.api_key_status.openai", &"set").unwrap();
        store.set_sync("llm.api_key_status", &"set").unwrap();
        assert_eq!(
            resolve_key_slot(&store),
            (
                "api_key_openai".to_string(),
                "llm.api_key_status.openai".to_string(),
            )
        );
    }

    /// Pre-scoping migration: `llm.provider` was persisted but the key was
    /// written to the legacy account — if the scoped flag is unset and the
    /// legacy one is set, the test follows the legacy slot so it stays
    /// consistent with what chat actually uses.
    #[test]
    fn resolve_key_slot_falls_back_to_legacy_for_pre_scoping_installs() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.provider", &"nvidia_nim").unwrap();
        // Scoped flag absent, legacy flag present → legacy slot.
        store.set_sync("llm.api_key_status", &"set").unwrap();
        assert_eq!(
            resolve_key_slot(&store),
            ("api_key".to_string(), "llm.api_key_status".to_string())
        );
    }

    /// A stale `llm.provider = "custom"` value still routes to the legacy
    /// slot (custom is supposed to delete the key, but guard anyway).
    #[test]
    fn resolve_key_slot_handles_stale_custom_provider() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.provider", &"custom").unwrap();
        assert_eq!(
            resolve_key_slot(&store),
            ("api_key".to_string(), "llm.api_key_status".to_string())
        );
    }
}
