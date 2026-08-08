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
    let host = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    if host.contains("anthropic.com") {
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
    } else if host.contains("integrate.api.nvidia.com") {
        LLMProvider::NvidiaNim
    } else {
        LLMProvider::OpenAI
    }
}

/// Distinct reasons we may fail to build a provider config. Splitting these
/// out lets the chat commands return a *specific* message ("set your API key")
/// rather than the orchestrator wandering into its `ANTHROPIC_API_KEY` env-var
/// fallback and failing with a confusing error far from the cause.
pub(crate) enum ProviderConfigError {
    /// No API key in the keyring and no legacy plaintext key to migrate.
    MissingApiKey,
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
    // An explicit provider key, if the user (or a future settings UI) set one,
    // overrides URL-based inference.
    let explicit_provider = state
        .store
        .get::<String>("llm.provider")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());

    let api_key = keyring::Entry::new("athena", "api_key")
        .ok()
        .and_then(|e| e.get_password().ok())
        .unwrap_or_default();

    if api_key.is_empty() {
        // Try migrating any legacy plaintext key that predates the keyring
        // integration before giving up.
        if let Ok(Some(value)) = state.store.get::<String>("llm.api_key") {
            if !value.is_empty() && value != "not_set" && value != "set" {
                if let Ok(entry) = keyring::Entry::new("athena", "api_key") {
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
                // duplicating the config-assembly logic below.
                return build_provider_config_from_store(state);
            }
        }
        log::warn!("No API key configured for LLM provider");
        return Err(ProviderConfigError::MissingApiKey);
    }

    let model = state
        .store
        .get::<String>("llm.model")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gpt-4o".to_string());
    let base_url = state
        .store
        .get::<String>("llm.base_url")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let provider = infer_provider(&base_url, explicit_provider.as_deref());

    Ok(athena_core::orchestrator::ProviderConfig::new(
        provider,
        api_key,
        model,
        String::new(),
        Some(base_url),
    ))
}
