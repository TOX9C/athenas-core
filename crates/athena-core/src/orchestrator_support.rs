//! Provider configuration, request safety helpers, and rate limiting for the orchestrator.

use super::OrchestratorError;
use crate::tool_executor::ToolInput;
use crate::types::{ImageData, LLMProvider};
use std::sync::Arc;

#[derive(Clone)]
pub struct ProviderConfig {
    pub provider: LLMProvider,
    api_key: secrecy::SecretString,
    pub model: String,
    pub system_prompt: String,
    pub base_url: Option<String>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("provider", &self.provider)
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("system_prompt", &"[...]")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl std::fmt::Display for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ProviderConfig(provider={:?}, model={}, base_url={:?})",
            self.provider, self.model, self.base_url
        )
    }
}

impl ProviderConfig {
    /// Create a new ProviderConfig with the given API key.
    pub fn new(
        provider: LLMProvider,
        api_key: impl Into<String>,
        model: String,
        system_prompt: String,
        base_url: Option<String>,
    ) -> Self {
        Self {
            provider,
            api_key: secrecy::SecretString::from(api_key.into()),
            model,
            system_prompt,
            base_url,
        }
    }

    /// Get the API key as a SecretString reference.
    pub fn api_key(&self) -> &secrecy::SecretString {
        &self.api_key
    }

    /// Get the API key exposure (use sparingly, only for actual API calls).
    pub fn expose_api_key(&self) -> &String {
        use secrecy::ExposeSecret;
        self.api_key.expose_secret()
    }
}

impl serde::Serialize for ProviderConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::Serialize;

        #[derive(Serialize)]
        struct ProviderConfigSer<'a> {
            provider: &'a LLMProvider,
            api_key: &'a str,
            model: &'a str,
            system_prompt: &'a str,
            base_url: &'a Option<String>,
        }

        let ser = ProviderConfigSer {
            provider: &self.provider,
            api_key: "[REDACTED]",
            model: &self.model,
            system_prompt: "[...]",
            base_url: &self.base_url,
        };
        ser.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ProviderConfigDe {
            provider: LLMProvider,
            api_key: String,
            model: String,
            system_prompt: String,
            base_url: Option<String>,
        }

        let de = ProviderConfigDe::deserialize(deserializer)?;
        Ok(ProviderConfig {
            provider: de.provider,
            api_key: secrecy::SecretString::from(de.api_key),
            model: de.model,
            system_prompt: de.system_prompt,
            base_url: de.base_url,
        })
    }
}

/// Redact credential-shaped substrings from an arbitrary message.
///
/// This is the crate's central content-level redactor. It is applied to HTTP
/// error-response bodies before they become return values (see the call sites
/// in `orchestrator.rs`, `orchestrator_stream.rs`, and `llm_models.rs`), and it
/// is also reused by the Tauri logging backend as a safety-net filter so that
/// any `log::debug!`/`info!`/… that accidentally interpolates a secret is
/// scrubbed before the line is flushed to the log file or webview console.
///
/// Kept as a self-contained pure function (no traits, no allocations beyond
/// the result) so it can run inside a `fern` formatter closure.
pub fn sanitize_error_message(msg: &str) -> String {
    let patterns = [
        (r"sk-[a-zA-Z0-9]{20,}", "sk-[REDACTED]"),
        (r"x-api-key: [^\s]+", "x-api-key: [REDACTED]"),
        (r"Bearer [^\s]+", "Bearer [REDACTED]"),
        (r"api[_-]?key[:=][^\s]+", "api_key=[REDACTED]"),
    ];
    let mut result = msg.to_string();
    for (pat, repl) in &patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            result = re.replace_all(&result, *repl).to_string();
        }
    }
    result
}

pub(super) fn build_anthropic_content(
    text: &str,
    images: Option<&[ImageData]>,
) -> serde_json::Value {
    match images {
        None | Some(&[]) => serde_json::Value::String(text.to_string()),
        Some(imgs) => {
            let mut blocks = Vec::new();
            for img in imgs {
                blocks.push(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": img.media_type,
                        "data": img.base64
                    }
                }));
            }
            blocks.push(serde_json::json!({
                "type": "text",
                "text": text
            }));
            serde_json::Value::Array(blocks)
        }
    }
}

fn url_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest)?;
    let authority = after_scheme.split('/').next()?;
    let hostport = authority.rsplit('@').next()?;
    if let Some(rest) = hostport.strip_prefix('[') {
        // IPv6 literal: host is everything up to the closing ']'.
        let v6 = rest.split(']').next().unwrap_or("");
        return Some(v6.to_string());
    }
    Some(hostport.split(':').next().unwrap_or("").to_string())
}

pub(crate) fn validate_base_url(url: &str) -> Result<(), OrchestratorError> {
    // Accept either scheme. We enforce HTTPS for any host that isn't a
    // loopback / private address, so an API key is never sent in cleartext
    // over the public internet — but local LLM servers (LM Studio, Ollama,
    // vLLM, …) documented as `http://localhost:1234/v1` still work.
    let (scheme, _rest) = url.split_once("://").ok_or_else(|| {
        OrchestratorError::Generic(
            "Base URL must include a scheme (https:// or http://)".to_string(),
        )
    })?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(OrchestratorError::Generic(format!(
            "Base URL must use http:// or https:// (got '{scheme}://')"
        )));
    }

    let host = url_host(url).unwrap_or_default();
    if host.is_empty() {
        return Err(OrchestratorError::Generic(
            "Base URL must have a valid hostname".to_string(),
        ));
    }

    // A loopback IP (IPv4 or IPv6) or the "localhost" label identifies a
    // local server and is exempt from the HTTPS requirement.
    let is_loopback = host
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
        || host == "localhost";

    if scheme == "http" && !is_loopback {
        return Err(OrchestratorError::Generic(
            "Base URL must use HTTPS for non-local hosts".to_string(),
        ));
    }

    // Public hostnames need a dot (e.g. api.openai.com). Single-label names
    // other than localhost are almost certainly a typo.
    if !is_loopback && !host.contains('.') {
        return Err(OrchestratorError::Generic(
            "Base URL must have a valid hostname".to_string(),
        ));
    }
    Ok(())
}

pub(super) struct RateLimiter {
    semaphore: Arc<tokio::sync::Semaphore>,
    min_interval: std::time::Duration,
}

impl RateLimiter {
    pub(super) fn new(min_interval_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            min_interval: std::time::Duration::from_millis(min_interval_ms),
        }
    }

    pub(super) async fn wait_if_needed(&self) {
        // Acquire the single permit. If it's currently held (by a
        // recent call's refiller task), this awaits until that task
        // releases it. The semaphore is never closed, so `acquire`
        // cannot fail in practice; we still handle the error path to
        // satisfy the type system and surface unexpected closes loudly.
        let permit = match self.semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                // The semaphore was closed (should never happen during
                // normal operation). Fall back to a no-op wait so we
                // don't panic in a request hot path.
                return;
            }
        };
        // Spawn a task that returns the permit after `min_interval`.
        // Because we used `acquire_owned`, the permit can move into
        // the spawned task and be dropped there — releasing the
        // semaphore slot — without us needing to hold it across the
        // caller's actual work.
        let interval = self.min_interval;
        tokio::spawn(async move {
            tokio::time::sleep(interval).await;
            drop(permit);
        });
    }
}

pub(super) fn build_openai_content(text: &str, images: Option<&[ImageData]>) -> serde_json::Value {
    match images {
        None | Some(&[]) => serde_json::Value::String(text.to_string()),
        Some(imgs) => {
            let mut parts = Vec::new();
            for img in imgs {
                let url = format!("data:{};base64,{}", img.media_type, img.base64);
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": url }
                }));
            }
            parts.push(serde_json::json!({
                "type": "text",
                "text": text
            }));
            serde_json::Value::Array(parts)
        }
    }
}

pub(super) fn json_to_tool_input(
    value: &serde_json::Value,
) -> Result<ToolInput, OrchestratorError> {
    serde_json::from_value(value.clone()).map_err(OrchestratorError::SerializationError)
}

pub(super) fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}

pub(super) fn heuristic_fallback_title(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let skip: &[&str] = &[
        "a",
        "an",
        "the",
        "and",
        "or",
        "but",
        "to",
        "of",
        "in",
        "on",
        "at",
        "for",
        "with",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "do",
        "does",
        "did",
        "can",
        "could",
        "will",
        "would",
        "should",
        "may",
        "might",
        "must",
        "shall",
        "i",
        "you",
        "we",
        "they",
        "it",
        "this",
        "that",
        "these",
        "those",
        "my",
        "your",
        "our",
        "their",
        "its",
        "please",
        "hey",
        "so",
        "then",
        "now",
        "just",
        "only",
        "also",
        "very",
        "really",
        "actually",
        "basically",
        "literally",
        "definitely",
        "probably",
        "maybe",
    ];

    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let content_words: Vec<&str> = words
        .iter()
        .filter(|w| !skip.contains(w) && w.len() > 1)
        .copied()
        .collect();

    let title = content_words
        .iter()
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "working".to_string()
    } else {
        trimmed.to_string()
    }
}
