use super::store::api_key_target;
use crate::state::AppState;

/// Infer the LLM provider from the configured base URL.
///
/// The Settings UI only collects a base URL + model (and an API key stored in
/// the keyring) — there is no explicit provider picker. Historically the
/// backend read a `llm.provider` key that nobody ever wrote, so every user was
/// silently routed through the OpenAI transport even when they meant to talk
/// to Anthropic. We now infer the provider from the host when the user hasn't
/// explicitly set `llm.provider`, and treat anything OpenAI-compatible
/// (Groq, OpenRouter, Together, local servers, …) as OpenAI.
fn infer_provider(base_url: &str, explicit: Option<&str>) -> athena_core::types::LLMProvider {
    use athena_core::types::LLMProvider;
    if let Some(p) = explicit {
        return match p.trim().to_ascii_lowercase().as_str() {
            "anthropic" => LLMProvider::Anthropic,
            "nvidia_nim" | "nvidia" | "nim" => LLMProvider::NvidiaNim,
            "lmstudio" | "lm_studio" | "lm-studio" => LLMProvider::Lmstudio,
            _ => LLMProvider::OpenAI,
        };
    }
    let host = url_host(base_url);
    if host_matches(&host, "anthropic.com") {
        LLMProvider::Anthropic
    } else if host == "localhost" && base_url.contains(":1234")
        || host.ends_with(".local")
        || (host == "localhost" && base_url.contains("/v1"))
    {
        // Heuristic: LM Studio's default port is 1234; Ollama lives on 11434.
        // We can't distinguish them perfectly, but LM Studio is the only
        // built-in provider with special behaviour (no vision), and it's the
        // one documented in the Settings placeholder.
        LLMProvider::Lmstudio
    } else if host_matches(&host, "integrate.api.nvidia.com") {
        LLMProvider::NvidiaNim
    } else {
        LLMProvider::OpenAI
    }
}

/// Extract the lowercase host part of a URL-ish string.
fn url_host(base_url: &str) -> String {
    let host = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split('/').next().unwrap_or(host);
    host.split(':').next().unwrap_or(host).to_ascii_lowercase()
}

/// True when `host` is exactly `domain` or a subdomain of it. The dotted
/// boundary prevents lookalike hosts such as `anthropic.com.evil.example`.
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// True when the keyring API key may be attached to a request to `base_url`.
///
/// The base URL is renderer-writable (`store_set` on `llm.base_url`), so a
/// compromised renderer could point it at an attacker host and have the
/// keyring secret sent along. Keys therefore only leave the process for the
/// known, pinned provider endpoints; custom base URLs (local servers, private
/// gateways) get no auth header — local servers need none, and anything else
/// must not receive the user's cloud key.
pub(crate) fn key_allowed_for_base_url(base_url: &str) -> bool {
    const PINNED_HOSTS: &[&str] = &[
        "api.openai.com",
        "anthropic.com",
        "integrate.api.nvidia.com",
    ];
    let host = url_host(base_url);
    PINNED_HOSTS.iter().any(|d| host_matches(&host, d))
}

/// Distinct reasons we may fail to build a provider config. Splitting these
/// out lets the chat commands return a *specific* message ("set your API key")
/// rather than the orchestrator wandering into its `ANTHROPIC_API_KEY` env-var
/// fallback and failing with a confusing error far from the cause.
#[derive(Clone)]
pub(crate) enum ProviderConfigError {
    /// No API key in the keyring and no legacy plaintext key to migrate.
    MissingApiKey,
}

/// List the models available from an OpenAI-compatible `/models` endpoint.
///
/// Returns a serialized JSON **string** (`{"ok": bool, "models": [...], "message": "..."}`)
/// following the `test_llm_api_key` string-return convention — the frontend
/// bridge casts the IPC result to a String and parses it with serde_json.
///
/// Key resolution: the freshly-typed key (passed from the Settings input) wins
/// so "Fetch models" works before the user hits Save; otherwise the keyring is
/// read from the slot the chat backend would use — provider-scoped when a
/// preset id is passed, legacy otherwise. With no key at all the request is
/// still attempted without an auth header (local servers like LM Studio need
/// none) and a cloud 401 surfaces as the error message.
#[tauri::command]
pub async fn llm_list_models(
    base_url: String,
    api_key: Option<String>,
    provider: Option<String>,
) -> Result<String, String> {
    let resolved_key = match api_key {
        Some(key) if !key.trim().is_empty() => Some(key.trim().to_string()),
        _ => {
            // Provider-scoped keyring account first, then the legacy account
            // (migration fallback for installs that persisted `llm.provider`
            // before per-provider key scoping shipped).
            let mut resolved = None;
            if let Some(p) = provider
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty() && *p != "custom")
            {
                if let Some((account, _)) = api_key_target(&format!("llm.api_key.{p}")) {
                    resolved = keyring::Entry::new("athena", &account)
                        .ok()
                        .and_then(|entry| entry.get_password().ok());
                }
            }
            if resolved.as_ref().is_none_or(|k| k.is_empty()) {
                resolved = keyring::Entry::new("athena", "api_key")
                    .ok()
                    .and_then(|entry| entry.get_password().ok());
            }
            resolved.filter(|key| !key.is_empty())
        }
    };
    // A freshly-typed key is a deliberate user gesture in the Settings dialog,
    // but a keyring-resolved key must not ride arbitrary `llm.base_url` values
    // (renderer-writable) to an unknown host — see `key_allowed_for_base_url`.
    let resolved_key =
        resolved_key.filter(|_| key_allowed_for_base_url(&base_url));

    let result = athena_core::llm_models::list_models(&base_url, resolved_key.as_deref()).await;
    let payload = match result {
        Ok(models) => serde_json::json!({
            "ok": true,
            "models": models,
            "message": format!("Found {} model(s)", models.len()),
        }),
        Err(message) => serde_json::json!({
            "ok": false,
            "models": [],
            "message": message,
        }),
    };
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

/// Build provider config from the persistent store for LLM API calls.
///
/// Returns `Ok(config)` when everything needed is present, or
/// `Err(MissingApiKey)` when the user hasn't set a key. Other misconfiguration
/// (unknown provider string) logs a warning and falls back to a sensible
/// default instead of blocking all chat.
pub(crate) fn build_provider_config_from_store(
    state: &AppState,
) -> Result<athena_core::orchestrator::ProviderConfig, ProviderConfigError> {
    // Cache check FIRST: the store revision moves on every mutation, so a
    // per-message rebuild only happens when configuration (or the keyring
    // generation below) actually changed.
    let revision = state.store.revision();
    let generation = PROVIDER_KEYRING_GENERATION.load(std::sync::atomic::Ordering::Acquire);
    if let Some((cached_revision, cached_generation, cached)) = PROVIDER_CONFIG_CACHE
        .lock()
        .as_ref()
        .filter(|(r, g, _)| *r == revision && *g == generation)
    {
        debug_assert_eq!((*cached_revision, *cached_generation), (revision, generation));
        return cached.clone();
    }
    let config = build_provider_config_uncached(state);
    // Only publish under the (revision, generation) we built from: if a
    // concurrent mutation moved either key while we were reading, a stale
    // entry under old keys would never hit anyway, so publishing is safe.
    *PROVIDER_CONFIG_CACHE.lock() = Some((revision, generation, config.clone()));
    config
}

/// Cached provider config, keyed by (store revision, keyring generation).
///
/// The store revision bumps on every `set`/`delete`/`set_sync`/`delete_sync`
/// — covering all `llm.*` writes from `store_set`/`store_delete` — so the
/// only keyring change that can bypass it is a direct keychain write, which
/// callers making such writes must signal via
/// [`invalidate_provider_config_cache`]. Out-of-band keychain edits (another
/// app) stay cached until the next store mutation or invalidation; the
/// config is re-verified on the next restart regardless.
type ProviderConfigCache = Option<(
    u64,
    u64,
    Result<athena_core::orchestrator::ProviderConfig, ProviderConfigError>,
)>;

static PROVIDER_CONFIG_CACHE: std::sync::LazyLock<parking_lot::Mutex<ProviderConfigCache>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// Monotonic counter bumped whenever a command writes/deletes a keyring
/// credential. Store writes already move the store revision and need no
/// explicit bump.
static PROVIDER_KEYRING_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Invalidate the cached provider config after a keyring credential write
/// (e.g. `store_api_key`/`clear_api_key`), which does not touch the store
/// and therefore does not move the store revision.
pub(crate) fn invalidate_provider_config_cache() {
    PROVIDER_KEYRING_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// Uncached build: reads the store and the keyring, migrates legacy
/// plaintext keys once, and assembles the final [`ProviderConfig`].
fn build_provider_config_uncached(
    state: &AppState,
) -> Result<athena_core::orchestrator::ProviderConfig, ProviderConfigError> {
    // The persisted provider id is authoritative: preset ids (openai,
    // anthropic, nvidia_nim, lmstudio) route to their own scoped keys so each
    // provider keeps its own base URL / model / API key. An empty value
    // (custom, or nothing saved yet) falls back to the legacy slots and URL
    // inference — preserving existing users' config exactly as before.
    let explicit_provider = state
        .store
        .get::<String>("llm.provider")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());
    let scoped = explicit_provider
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "custom");

    let (keyring_account, model_key, base_url_key) = match scoped {
        Some(provider) => (
            format!("api_key_{provider}"),
            format!("llm.model.{provider}"),
            format!("llm.base_url.{provider}"),
        ),
        None => (
            "api_key".to_string(),
            "llm.model".to_string(),
            "llm.base_url".to_string(),
        ),
    };

    let mut api_key = keyring::Entry::new("athena", &keyring_account)
        .ok()
        .and_then(|e| e.get_password().ok())
        .unwrap_or_default();
    if api_key.is_empty() && scoped.is_some() {
        // Migration fallback: pre-scoping installs persisted `llm.provider`
        // but stored the key in the legacy account.
        api_key = keyring::Entry::new("athena", "api_key")
            .ok()
            .and_then(|e| e.get_password().ok())
            .unwrap_or_default();
    }

    if api_key.is_empty() {
        // Try migrating any legacy plaintext key that predates the keyring
        // integration before giving up. Only relevant for the legacy slot —
        // provider-scoped keys were never stored in plaintext.
        if scoped.is_none() {
            if let Ok(Some(value)) = state.store.get::<String>("llm.api_key") {
                if !value.is_empty() && value != "not_set" && value != "set" {
                    if let Ok(entry) = keyring::Entry::new("athena", &keyring_account) {
                        // Only delete the legacy plaintext key after the keyring
                        // write is confirmed. If set_password fails we recurse
                        // without deleting — the plaintext key stays in the store
                        // so the recursive call can find it again instead of
                        // returning MissingApiKey and losing the key entirely.
                        if entry.set_password(&value).is_ok() {
                            let _ = state.store.delete_sync("llm.api_key");
                        }
                    }
                    // Recurse once to pick up the freshly-migrated key without
                    // duplicating the config-assembly logic below. The store
                    // delete above bumped the revision, so a cached entry
                    // from before the migration can never hit.
                    return build_provider_config_uncached(state);
                }
            }
        }
        log::warn!("No API key configured for LLM provider");
        return Err(ProviderConfigError::MissingApiKey);
    }

    // Scoped value wins; the legacy slot is a migration fallback for installs
    // that saved before per-provider scoping shipped (same rationale as the
    // API-key fallback above).
    let legacy_model = || {
        state
            .store
            .get::<String>("llm.model")
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    };
    let legacy_base_url = || {
        state
            .store
            .get::<String>("llm.base_url")
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    };
    let model = state
        .store
        .get::<String>(&model_key)
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .or_else(legacy_model)
        .unwrap_or_else(|| "gpt-4o".to_string());
    let base_url = state
        .store
        .get::<String>(&base_url_key)
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .or_else(legacy_base_url)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let provider = infer_provider(&base_url, explicit_provider.as_deref());

    // Pin the keyring secret to known provider hosts: `llm.base_url` is
    // writable via `store_set`, so without this a compromised renderer could
    // redirect the chat transport and exfiltrate the key. Custom/local URLs
    // proceed with no auth header (see `key_allowed_for_base_url`).
    let api_key = if key_allowed_for_base_url(&base_url) {
        api_key
    } else {
        log::warn!(
            "Provider base URL '{}' is not a pinned provider host; sending no API key",
            base_url
        );
        String::new()
    };

    Ok(athena_core::orchestrator::ProviderConfig::new(
        provider,
        api_key,
        model,
        String::new(),
        Some(base_url),
    ))
}
