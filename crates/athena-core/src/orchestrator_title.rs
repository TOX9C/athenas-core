//! Title generation and retry handling for the orchestrator.

use super::orchestrator_support::heuristic_fallback_title;
use super::{
    AthenaOrchestrator, OrchestratorError, ANTHROPIC_VERSION, DEFAULT_ANTHROPIC_MODEL,
    DEFAULT_BACKOFF_DELAYS_MS,
};
use crate::types::LLMProvider;
use secrecy::ExposeSecret;

impl AthenaOrchestrator {
    /// Send a one-shot request to the configured LLM to summarize a prompt
    /// into a short title. Does NOT touch conversation history.
    ///
    /// Retries transient failures (network / 5xx / parse) with backoff up to
    /// `MAX_ATTEMPTS` times, then returns `Err`. A missing API key is
    /// non-retryable (returns `Err(MissingApiKey)` immediately).
    pub async fn summarize_title(&self, raw_prompt: &str) -> Result<String, OrchestratorError> {
        self.summarize_title_with_backoff(raw_prompt, DEFAULT_BACKOFF_DELAYS_MS)
            .await
    }

    /// Test seam: same as `summarize_title` but with a (near-zero) backoff
    /// schedule so retry tests don't sleep for seconds.
    #[cfg(test)]
    pub async fn summarize_title_for_test(
        &self,
        raw_prompt: &str,
    ) -> Result<String, OrchestratorError> {
        self.summarize_title_with_backoff(raw_prompt, &[1u64, 1, 1])
            .await
    }

    async fn summarize_title_with_backoff(
        &self,
        raw_prompt: &str,
        backoff_delays_ms: &[u64],
    ) -> Result<String, OrchestratorError> {
        const SYSTEM: &str = "You write short, descriptive titles for coding sessions based on the user's first prompt.\nWrite a short sentence in sentence case describing what the agent is doing, 1-6 words.\nUse the imperative or -ing form (e.g. \"analyzing the codebase\", \"checking rust version\", \"fixing the login bug\").\nNo quotes, no trailing punctuation, no preamble - output only the title.";
        let prompt = raw_prompt.to_string();

        let config = { self.provider_config.lock().as_ref().cloned() };

        let (provider, api_key, model, base_url) = match config {
            Some(c) => (
                c.provider.clone(),
                c.api_key().clone(),
                c.model.clone(),
                c.base_url.clone(),
            ),
            None => {
                let api_key = std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .ok_or(OrchestratorError::MissingApiKey)?;
                (
                    LLMProvider::Anthropic,
                    secrecy::SecretString::from(api_key),
                    DEFAULT_ANTHROPIC_MODEL.to_string(),
                    None,
                )
            }
        };

        let client = &self.http_client;

        let mut last_err: Option<OrchestratorError> = None;
        for (attempt, &delay_ms) in backoff_delays_ms.iter().enumerate() {
            match self
                .one_title_call(
                    client,
                    provider.clone(),
                    api_key.clone(),
                    model.clone(),
                    base_url.clone(),
                    SYSTEM,
                    &prompt,
                )
                .await
            {
                Ok(title) => {
                    // Safety guard: if the LLM returns the raw prompt (or a
                    // huge chunk of it) instead of a short title, log it and
                    // fall back to a heuristic so the user never sees the
                    // entire prompt as the pane label.
                    let cleaned = title.trim();
                    let raw_trimmed = raw_prompt.trim();
                    // Safety guard: reject exact echo (case-insensitive) or
                    // anything unreasonably long for a 1–6 word title.
                    let is_echo = cleaned.eq_ignore_ascii_case(raw_trimmed);
                    let is_too_long = cleaned.chars().count() > 60;

                    if !cleaned.is_empty() && !is_echo && !is_too_long {
                        log::debug!("[summarize_title] attempt={} title='{}'", attempt, cleaned);
                        return Ok(cleaned.to_string());
                    }

                    log::warn!(
                        "[summarize_title] attempt={} LLM echoed raw prompt or returned long text (len={}). Falling back to heuristic.",
                        attempt,
                        cleaned.len()
                    );
                    return Ok(heuristic_fallback_title(raw_prompt));
                }
                Err(e) => {
                    // MissingApiKey is non-retryable; propagate immediately.
                    if matches!(e, OrchestratorError::MissingApiKey) {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
            // Back off before the next attempt (skip after the last attempt).
            if attempt + 1 < backoff_delays_ms.len() {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
        Err(last_err.unwrap_or_else(|| {
            OrchestratorError::Generic("title generation failed with no attempts".to_string())
        }))
    }

    /// Single LLM call for title summarization. Returns the parsed title on
    /// success, or an error the caller decides whether to retry.
    #[allow(clippy::too_many_arguments)]
    async fn one_title_call(
        &self,
        client: &reqwest::Client,
        provider: LLMProvider,
        api_key: secrecy::SecretString,
        model: String,
        base_url: Option<String>,
        system: &str,
        prompt: &str,
    ) -> Result<String, OrchestratorError> {
        match provider {
            LLMProvider::Anthropic => {
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": 48,
                    "temperature": 0,
                    "system": system,
                    "messages": [{"role": "user", "content": prompt}]
                });
                let response = client
                    .post(format!("{}/messages", self.anthropic_base_url))
                    .header("x-api-key", api_key.expose_secret())
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response.text().await.unwrap_or_default();
                    return Err(OrchestratorError::Generic(format!(
                        "Anthropic API error {}: {}",
                        status, err_text
                    )));
                }
                let json: serde_json::Value = response.json().await?;
                let content = json["content"].as_array().ok_or_else(|| {
                    OrchestratorError::Generic(
                        "Invalid Anthropic response: no content array".to_string(),
                    )
                })?;
                let summary = content
                    .iter()
                    .filter(|b| b["type"].as_str() == Some("text"))
                    .map(|b| b["text"].as_str().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                Ok(summary)
            }
            LLMProvider::OpenAI | LLMProvider::NvidiaNim | LLMProvider::Lmstudio => {
                let url = match &provider {
                    LLMProvider::NvidiaNim => "https://integrate.api.nvidia.com/v1".to_string(),
                    LLMProvider::OpenAI => "https://api.openai.com/v1".to_string(),
                    LLMProvider::Lmstudio => {
                        base_url.unwrap_or_else(|| "http://localhost:1234/v1".to_string())
                    }
                    _ => unreachable!(),
                };
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": 48,
                    "temperature": 0,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": prompt}
                    ],
                    "stream": false,
                });
                let response = client
                    .post(format!("{}/chat/completions", url))
                    .header(
                        "Authorization",
                        format!("Bearer {}", api_key.expose_secret()),
                    )
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response.text().await.unwrap_or_default();
                    return Err(OrchestratorError::Generic(format!(
                        "OpenAI API error {}: {}",
                        status, err_text
                    )));
                }
                let json: serde_json::Value = response.json().await?;
                let summary = json["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("Summary")
                    .trim()
                    .to_string();
                Ok(summary)
            }
        }
    }
}
