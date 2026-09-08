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

/// Classify a non-success provider HTTP response into an orchestrator error.
///
/// 410 Gone — and 404 when the (already sanitized) body names a model — mean
/// the configured model was retired upstream. Those become
/// [`OrchestratorError::ModelUnavailable`] so the UI can send the user to
/// Settings instead of showing a bare toast. Everything else keeps the
/// historical `"{provider} API error {status}: {body}"` shape.
pub(super) fn classify_api_error(
    provider: &str,
    status: reqwest::StatusCode,
    sanitized: String,
) -> OrchestratorError {
    if status == reqwest::StatusCode::GONE
        || (status == reqwest::StatusCode::NOT_FOUND
            && sanitized.to_lowercase().contains("model"))
    {
        OrchestratorError::ModelUnavailable {
            status: status.as_u16(),
            detail: sanitized,
        }
    } else {
        OrchestratorError::Generic(format!("{provider} API error {status}: {sanitized}"))
    }
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

#[cfg(test)]
mod tests {
    use super::classify_api_error;
    use crate::OrchestratorError;
    use reqwest::StatusCode;

    #[test]
    fn gone_is_model_unavailable() {
        match classify_api_error("OpenAI", StatusCode::GONE, "retired".into()) {
            OrchestratorError::ModelUnavailable { status, .. } => assert_eq!(status, 410),
            other => panic!("expected ModelUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn model_scoped_not_found_is_unavailable_but_plain_404_is_generic() {
        assert!(matches!(
            classify_api_error("OpenAI", StatusCode::NOT_FOUND, "model not found".into()),
            OrchestratorError::ModelUnavailable { status: 404, .. }
        ));
        let generic = classify_api_error("Anthropic", StatusCode::NOT_FOUND, "unknown endpoint".into());
        assert!(
            matches!(generic, OrchestratorError::Generic(_)),
            "plain 404 keeps the legacy generic shape: {generic:?}"
        );
        assert!(generic.to_string().contains("Anthropic API error 404"));
    }

    #[test]
    fn rate_limit_and_server_errors_stay_generic() {
        for code in [401u16, 429, 500] {
            let err = classify_api_error(
                "Anthropic",
                StatusCode::from_u16(code).unwrap(),
                "boom".into(),
            );
            assert!(
                matches!(err, OrchestratorError::Generic(_)),
                "{code} must stay generic: {err:?}"
            );
        }
    }

    #[test]
    fn sanitize_redacts_every_credential_pattern() {
        let raw = concat!(
            "sk-AbCdEfGhIjKlMnOpQrSt ",
            "x-api-key: sekret123 ",
            "Bearer to.ken ",
            "api_key=supersecret ",
            "apikey:alsosupersecret ",
        );
        let out = super::sanitize_error_message(raw);
        assert!(out.contains("sk-[REDACTED]"), "{out}");
        assert!(out.contains("x-api-key: [REDACTED]"), "{out}");
        assert!(out.contains("Bearer [REDACTED]"), "{out}");
        assert!(out.contains("api_key=[REDACTED]"), "{out}");
        for secret in [
            "AbCdEfGhIjKlMnOpQrSt",
            "sekret123",
            "to.ken",
            "supersecret",
            "alsosupersecret",
        ] {
            assert!(!out.contains(secret), "secret leaked: {secret}");
        }
    }

    #[test]
    fn sanitize_leaves_plain_messages_untouched() {
        let msg = "rate limit exceeded, retry after 30s";
        assert_eq!(super::sanitize_error_message(msg), msg);
        // Short sk- fragments below the 20-char threshold are not keys.
        let short = "prefix sk-short suffix";
        assert_eq!(super::sanitize_error_message(short), short);
    }

    #[test]
    fn url_host_handles_userinfo_port_and_ipv6() {
        assert_eq!(super::url_host("https://user:pw@example.com:8443/v1").as_deref(), Some("example.com"));
        assert_eq!(super::url_host("http://[::1]:11434/v1").as_deref(), Some("::1"));
        assert_eq!(super::url_host("https://[2001:db8::1]/v1").as_deref(), Some("2001:db8::1"));
        assert_eq!(super::url_host("no-scheme"), None);
    }

    #[test]
    fn validate_base_url_accepts_ipv6_and_ip_loopback_http() {
        assert!(super::validate_base_url("http://[::1]:11434/v1").is_ok());
        assert!(super::validate_base_url("http://127.0.0.1:1234/v1").is_ok());
    }

    #[test]
    fn validate_base_url_rejects_single_label_and_private_ip_http() {
        // Public HTTPS hosts need a dot; a bare "ollama" label is a typo.
        assert!(super::validate_base_url("https://ollama").is_err());
        // Private but non-loopback IPs (LAN) are not loopback: plain HTTP
        // would send the key in cleartext across the LAN, so it is rejected.
        assert!(super::validate_base_url("http://192.168.1.50:11434/v1").is_err());
    }

    #[test]
    fn anthropic_content_is_plain_string_without_images() {
        for images in [None, Some(vec![])] {
            let v = super::build_anthropic_content("hello", images.as_deref());
            assert_eq!(v, serde_json::json!("hello"));
        }
    }

    #[test]
    fn anthropic_content_builds_image_blocks_then_text() {
        let img = crate::types::ImageData {
            base64: "QUJD".into(),
            media_type: "image/png".into(),
        };
        let v = super::build_anthropic_content("hi", Some(&[img]));
        let blocks = v.as_array().expect("images produce a block array");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        assert_eq!(blocks[0]["source"]["data"], "QUJD");
        assert_eq!(blocks[1], serde_json::json!({"type": "text", "text": "hi"}));
    }

    #[test]
    fn openai_content_wraps_image_as_data_url() {
        let img = crate::types::ImageData {
            base64: "QUJD".into(),
            media_type: "image/jpeg".into(),
        };
        let v = super::build_openai_content("hi", Some(&[img]));
        let parts = v.as_array().expect("images produce parts");
        assert_eq!(parts[0]["image_url"]["url"], "data:image/jpeg;base64,QUJD");
        assert_eq!(parts[1], serde_json::json!({"type": "text", "text": "hi"}));
        // No images → plain string, matching Anthropic behaviour.
        assert_eq!(super::build_openai_content("hi", None), serde_json::json!("hi"));
    }

    #[test]
    fn fallback_title_types_stopwords_and_punctuation() {
        assert_eq!(
            super::heuristic_fallback_title("Hey, please fix the broken build!"),
            "fix broken build"
        );
        // Stopwords-only input never yields an empty title.
        assert_eq!(super::heuristic_fallback_title("the an a"), "working");
        assert_eq!(super::heuristic_fallback_title("!!!"), "working");
        // At most four content words.
        assert_eq!(
            super::heuristic_fallback_title("alpha beta gamma delta epsilon zeta"),
            "alpha beta gamma delta"
        );
    }
}
