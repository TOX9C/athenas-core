//! Fetch the available models from an OpenAI-compatible `/models` endpoint.
//!
//! This is used by the Settings UI to populate a model dropdown after the
//! user picks a provider (NVIDIA NIM, LM Studio, a custom endpoint, …) and
//! enters a base URL. The request runs on the backend because the API key
//! lives in the OS keychain (the frontend only ever sees "set"/"not_set")
//! and because WASM cannot make cross-origin requests to arbitrary hosts.

use crate::orchestrator::{sanitize_error_message, validate_base_url};
use std::time::Duration;

/// Query `{base_url}/models` and return the list of model IDs.
///
/// The `Authorization: Bearer` header is attached **only** when `api_key` is
/// `Some` and non-empty — local OpenAI-compatible servers (LM Studio, Ollama)
/// need no key, so a missing key must not block the fetch; a cloud provider
/// without a key surfaces its 401 as the returned error.
pub async fn list_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>, String> {
    // Mirror the chat path's SSRF guard: https for public hosts, http allowed
    // only for loopback. Error text is sanitized before it can reach the UI.
    validate_base_url(base_url).map_err(|e| sanitize_error_message(&e.to_string()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let mut request = client.get(&url);
    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        request = request.header("Authorization", format!("Bearer {}", key.trim()));
    }

    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to list models ({}): {}",
            status,
            sanitize_error_message(&body)
        ));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let data = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Model list response did not contain a data array".to_string())?;
    let ids: Vec<String> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
        .filter(|s| !s.is_empty())
        .collect();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn model(id: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "object": "model" })
    }

    #[tokio::test]
    async fn list_models_returns_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [model("model-a"), model("model-b")]
            })))
            .mount(&server)
            .await;

        let base = format!("{}/v1", server.uri());
        let ids = list_models(&base, Some("test-key")).await.unwrap();
        assert_eq!(ids, vec!["model-a".to_string(), "model-b".to_string()]);
    }

    #[tokio::test]
    async fn list_models_without_key_omits_auth_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [model("local-model")]
            })))
            .mount(&server)
            .await;

        let ids = list_models(&server.uri(), None).await.unwrap();
        assert_eq!(ids, vec!["local-model".to_string()]);

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].headers.get("authorization").is_none(),
            "no-key request must not send an auth header"
        );
    }

    #[tokio::test]
    async fn list_models_surfaces_401_without_echoing_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                r#"{"error":{"message":"invalid key","header":"Authorization: Bearer sk-secret-1234567890"}}"#,
            ))
            .mount(&server)
            .await;

        let base = format!("{}/v1", server.uri());
        let err = list_models(&base, Some("sk-secret-1234567890"))
            .await
            .unwrap_err();
        assert!(err.contains("401"), "expected status in error, got: {err}");
        assert!(
            !err.contains("sk-secret-1234567890"),
            "key leaked into error: {err}"
        );
    }

    #[tokio::test]
    async fn list_models_rejects_non_data_shape() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": ["flat-array-shape"]
            })))
            .mount(&server)
            .await;

        let err = list_models(&server.uri(), None).await.unwrap_err();
        assert!(
            err.contains("data array"),
            "unexpected error for non-OpenAI shape: {err}"
        );
    }

    #[tokio::test]
    async fn list_models_accepts_http_loopback_only() {
        // A public http host must be rejected before any request is made.
        let err = list_models("http://api.example.com/v1", Some("key"))
            .await
            .unwrap_err();
        assert!(
            err.contains("HTTPS"),
            "expected HTTPS rejection, got: {err}"
        );
    }
}
