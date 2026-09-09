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
        // The keyring is the source of truth for key presence; the
        // `llm.api_key_status` flag is only a cache and is invalidated when
        // it contradicts the keyring (whole block extracted so tests can
        // inject a fake keyring result).
        let probe = probe_keyring(&account);
        return Ok(api_key_status(
            &state.store,
            &key,
            &account,
            &status_key,
            probe,
        ));
    }

    state
        .store
        .get::<String>(&key)
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Key '{}' not found", key)))
}

/// Result of probing the OS keyring for an API-key account.
///
/// The keyring is the source of truth for key presence; unlike a plain
/// `bool`, this distinguishes "definitively absent" from "couldn't read",
/// since an unreadable keychain (locked, permission denied) must not cause
/// the confirmation flag to be cleared — the flag is retained in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyringProbe {
    /// A non-empty key is present in the keyring.
    Present,
    /// The keyring answered authoritatively and there is no key.
    Missing,
    /// The keyring could not be read (locked keychain, permission error, …).
    Unreadable,
}

/// Read the keyring entry for `account` and classify the outcome.
fn probe_keyring(account: &str) -> KeyringProbe {
    match keyring::Entry::new("athena", account) {
        Ok(entry) => match entry.get_password() {
            Ok(key) if !key.is_empty() => KeyringProbe::Present,
            Ok(_) => KeyringProbe::Missing,
            Err(keyring::Error::NoEntry) => KeyringProbe::Missing,
            Err(e) => {
                log::warn!("[store_get] keyring read failed for '{account}': {e}");
                KeyringProbe::Unreadable
            }
        },
        Err(e) => {
            log::warn!("[store_get] failed to open keyring entry '{account}': {e}");
            KeyringProbe::Unreadable
        }
    }
}

/// Resolve the "set"/"not_set" answer for an API-key store key.
///
/// `probe` is the outcome of [`probe_keyring`] — the keyring is the source
/// of truth; the `llm.api_key_status[.<provider>]` flag is only a cache.
/// When the keyring authoritatively reports no key but the cached flag says
/// "set", the stale flag is invalidated so later reads don't resurrect it.
/// When the keyring can't be read at all, the cached flag is honored as the
/// best available answer (flag exists precisely to avoid keychain prompts
/// on panel mount).
fn api_key_status(
    store: &athena_store::KeyValueStore,
    key: &str,
    account: &str,
    status_key: &str,
    probe: KeyringProbe,
) -> String {
    let flag_set = matches!(&store.get::<String>(status_key), Ok(Some(s)) if s == "set");
    match probe {
        KeyringProbe::Present => {
            log::debug!("[store_get] {key} => found in OS keyring");
            // Repair the confirmation flag in case it was lost
            let _ = store.set_sync(status_key, &"set");
            return "set".to_string();
        }
        KeyringProbe::Missing => {
            if flag_set {
                log::warn!(
                    "[store_get] {key} => stale '{status_key}' flag contradicts \
                     empty keyring; invalidating cached flag"
                );
                let _ = store.delete_sync(status_key);
            }
        }
        KeyringProbe::Unreadable => {
            if flag_set {
                log::debug!("[store_get] {key} => keyring unreadable, confirmed by flag");
                return "set".to_string();
            }
        }
    }
    // Fallback: check store for a legacy plaintext key and migrate it
    if let Ok(Some(value)) = store.get::<String>(key) {
        if !value.is_empty() && value != "not_set" && value != "set" {
            // Found a raw key in the store — migrate to keyring
            if let Ok(entry) = keyring::Entry::new("athena", account) {
                let _ = entry.set_password(&value);
            }
            // Delete the plaintext key from the store so it never leaks again
            let _ = store.delete_sync(key);
            // Write the confirmation flag too
            let _ = store.set_sync(status_key, &"set");
            return "set".to_string();
        }
    }
    // Last fallback: the orchestrator itself accepts a bare
    // ANTHROPIC_API_KEY from the process environment when no keyring key
    // exists (see athena-core orchestrator_stream / orchestrator request
    // builders). Report "set" in that case too, so the composer doesn't
    // block chats that would actually work.
    if std::env::var("ANTHROPIC_API_KEY")
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
    {
        return "set".to_string();
    }
    "not_set".to_string()
}

/// Set a value in the persistent key-value store.
#[tauri::command]
pub async fn store_set(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
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
        .set(&key, &value)
        .await
        .map_err(|e| e.to_string())?;
    // User-scoped settings writes (llm.*, theme, fonts, …) are rare and small:
    // persist immediately instead of waiting for the graceful-shutdown flush,
    // which never runs when the process dies without ExitRequested (dev
    // Ctrl+C, SIGTERM, crash) — the change would silently revert on next
    // launch. `workspaces` is excluded: batch-written by the coalesced
    // save queue and keeps its deferred flush.
    if key != "workspaces" {
        if let Err(e) = state.store.flush_if_dirty().await {
            log::warn!("store_set: immediate flush of '{key}' failed: {e}");
        }
    }

    // Workspace writes (desktop or paired phone, same code path) broadcast a
    // change event so every other surface holding an in-memory copy reloads
    // instead of silently working from stale state. `workspace:changed` is in
    // the relay's event allowlist, so paired phones receive it too.
    if key == "workspaces" {
        use tauri::Emitter;
        let _ = app.emit(super::workspace::WORKSPACE_CHANGED_EVENT, value);
    }
    Ok(())
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
                let scoped_set =
                    matches!(store.get::<String>(&slot.1), Ok(Some(ref s)) if s == "set");
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

/// Recovery contract for stale API-key status flags.
///
/// `store_get` treats the keyring as the source of truth, but when the
/// keyring can't be read at all (locked keychain, permission prompt) it
/// answers from the persisted flag, so a keychain entry deleted out-of-band
/// can leave a stale `"set"` flag behind on the unreadable path. When a
/// chat call then fails at the provider with HTTP 401 — or
/// `ModelUnavailable`, which
/// is emitted for retired/upstream-removed configurations and treated the
/// same so the user is re-prompted — this clears the corresponding
/// `llm.api_key_status[.<provider>]` flag (resolved through
/// [`resolve_key_slot`], i.e. the same slot the chat path uses). The next
/// `store_get` then falls through to the real keyring check and reports
/// `"not_set"` instead of trusting the stale flag.
///
/// Called from the stream-event bridge (`state.rs`) so every chat surface —
/// desktop, relay phone, plugins — shares the recovery, and from the command
/// error paths in `athena.rs` for non-streaming calls that never produce a
/// stream event.
pub(crate) fn clear_api_key_flag_on_provider_error(
    store: &athena_store::KeyValueStore,
    model_unavailable: bool,
    message: &str,
) {
    if !model_unavailable && !message.contains("401") {
        return;
    }
    let (_, status_key) = resolve_key_slot(store);
    let _ = store.delete_sync(&status_key);
    log::warn!(
        "[store] cleared {status_key}: chat failed with model-unavailable/401; \
         Settings will re-prompt for the API key"
    );
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
    /// Regression: a stale `llm.api_key_status = "set"` flag whose keyring
    /// entry was deleted out-of-band must NOT be trusted — the keyring is
    /// the source of truth, so the flag is invalidated and `store_get`
    /// reports `"not_set"` (pre-fix the fast path returned `"set"` from the
    /// flag alone, contradicting the keyring). The env-var fallback is
    /// neutralized so the keyring verdict is what's observed.
    #[test]
    fn api_key_status_stale_set_flag_is_overruled_by_empty_keyring() {
        let prev_env = std::env::var_os("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.api_key_status", &"set").unwrap();
        let status = api_key_status(
            &store,
            "llm.api_key",
            "api_key",
            "llm.api_key_status",
            KeyringProbe::Missing,
        );
        if let Some(v) = prev_env {
            std::env::set_var("ANTHROPIC_API_KEY", v);
        }
        assert_eq!(status, "not_set");
        assert_eq!(
            store.get::<String>("llm.api_key_status").unwrap(),
            None,
            "contradicted sentinel must be invalidated"
        );
    }
    /// status flag, so the next `store_get` falls through to the keyring
    /// instead of trusting the stale "set" flag (out-of-band keychain
    /// deletion recovery contract).
    #[test]
    fn clear_api_key_flag_on_provider_error_clears_flag_on_401() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.api_key_status", &"set").unwrap();
        clear_api_key_flag_on_provider_error(
            &store,
            false,
            "Anthropic API error 401: invalid x-api-key",
        );
        assert_eq!(store.get::<String>("llm.api_key_status").unwrap(), None);
    }

    /// ModelUnavailable (retired/removed upstream configuration) is treated
    /// like a 401: the flag is cleared so Settings re-prompts.
    #[test]
    fn clear_api_key_flag_on_provider_error_clears_flag_on_model_unavailable() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.provider", &"openai").unwrap();
        store.set_sync("llm.api_key_status.openai", &"set").unwrap();
        clear_api_key_flag_on_provider_error(&store, true, "");
        assert_eq!(
            store.get::<String>("llm.api_key_status.openai").unwrap(),
            None,
            "scoped flag for the routed provider must be cleared"
        );
    }

    /// Success / unrelated failures leave the flag untouched — only
    /// auth-shaped failures trigger the recovery.
    #[test]
    fn clear_api_key_flag_on_provider_error_keeps_flag_on_other_errors() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.api_key_status", &"set").unwrap();
        clear_api_key_flag_on_provider_error(&store, false, "OpenAI API error 500: boom");
        clear_api_key_flag_on_provider_error(&store, false, "");
        assert_eq!(
            store.get::<String>("llm.api_key_status").unwrap(),
            Some("set".to_string())
        );
    }

    /// Non-API-key paths are untouched: the recovery only ever deletes the
    /// resolved status flag and leaves neighbouring keys alone.
    #[test]
    fn clear_api_key_flag_on_provider_error_leaves_other_keys_untouched() {
        let store = athena_store::KeyValueStore::new_empty();
        store.set_sync("llm.api_key_status", &"set").unwrap();
        store.set_sync("llm.model", &"gpt-4o").unwrap();
        store.set_sync("theme", &"dark").unwrap();
        clear_api_key_flag_on_provider_error(&store, false, "OpenAI API error 401: nope");
        assert_eq!(
            store.get::<String>("llm.model").unwrap(),
            Some("gpt-4o".to_string())
        );
        assert_eq!(
            store.get::<String>("theme").unwrap(),
            Some("dark".to_string())
        );
    }
}
