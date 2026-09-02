use super::provider_config::{build_provider_config_from_store, ProviderConfigError};
use crate::state::AppState;
use std::sync::Arc;
use tauri::State;
/// Convert an orchestrator error for the IPC boundary, clearing a stale
/// API-key status flag when the provider rejected the request (see
/// [`super::store::clear_api_key_flag_on_provider_error`]). Central so every
/// non-streaming chat command shares the recovery contract; streaming turns
/// additionally clear via the stream-event bridge in `state.rs`.
fn provider_chat_error(state: &AppState, error: athena_core::types::OrchestratorError) -> String {
    let model_unavailable =
        matches!(error, athena_core::types::OrchestratorError::ModelUnavailable { .. });
    let message = error.to_string();
    super::store::clear_api_key_flag_on_provider_error(
        &state.store,
        model_unavailable,
        &message,
    );
    message
}

/// Send a text message to the configured LLM provider and return the response.
#[tauri::command]
pub async fn athena_chat(state: State<'_, AppState>, message: String) -> Result<String, String> {
    if !state.rate_limiter.check("athena_chat") {
        return Err("Rate limit exceeded. Please wait a moment.".to_string());
    }
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    orchestrator
        .send_message(message, None)
        .await
        .map_err(|e| provider_chat_error(&state, e))
}

/// Start a request-scoped streaming chat turn. Chunks and lifecycle events are
/// emitted on `athena:stream`; the returned string is only a compatibility
/// fallback for callers that still await the command result.
#[tauri::command]
pub async fn athena_chat_stream(
    state: State<'_, AppState>,
    message: String,
    session_id: String,
    request_id: String,
) -> Result<String, String> {
    if request_id.trim().is_empty() {
        return Err("request_id is required".to_string());
    }
    if !state.rate_limiter.check("athena_chat_stream") {
        return Err("Rate limit exceeded. Please wait a moment.".to_string());
    }
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    let cancel = orchestrator
        .register_request(&request_id)
        .map_err(|e| e.to_string())?;
    orchestrator
        .stream_message(request_id, session_id, message, None, cancel)
        .await
        .map_err(|e| provider_chat_error(&state, e))
}

/// Cancel an in-flight streaming chat request.
#[tauri::command]
pub fn athena_cancel_stream(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<bool, String> {
    Ok(state.orchestrator.cancel_request(&request_id))
}

/// Send a text message to the LLM provider, associating it with a specific session.
#[tauri::command]
pub async fn athena_chat_with_session(
    state: State<'_, AppState>,
    message: String,
    session_id: String,
) -> Result<String, String> {
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    orchestrator
        .send_message_with_session(session_id, message, None)
        .await
        .map_err(|e| provider_chat_error(&state, e))
}

/// Send a message with image attachments to the LLM provider.
#[tauri::command]
pub async fn athena_chat_with_images(
    state: State<'_, AppState>,
    message: String,
    images: String,
) -> Result<String, String> {
    let image_data: Vec<athena_core::types::ImageData> =
        serde_json::from_str(&images).map_err(|e| e.to_string())?;
    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("API key is required. Please set it in Settings → Athena.".to_string());
        }
    }
    orchestrator
        .send_message(message, Some(image_data))
        .await
        .map_err(|e| provider_chat_error(&state, e))
}

pub(crate) fn prompt_is_sensitive(raw_prompt: &str) -> bool {
    let lowercase = raw_prompt.to_lowercase();
    let plaintext = [
        "password",
        "passw0rd",
        "p@ssword",
        "token",
        "t0ken",
        "t0k3n",
        "secret",
        "s3cret",
        "s3cr3t",
        "api_key",
        "apikey",
        "api-key",
        "api_k3y",
        "authorization",
        "auth",
        "4uth",
        "credential",
        "cr3dential",
        "private key",
        "passphrase",
        "pin",
    ];
    if plaintext.iter().any(|&kw| lowercase.contains(kw)) {
        return true;
    }
    let normalized = lowercase
        .replace('@', "a")
        .replace('0', "o")
        .replace('3', "e")
        .replace(['1', '!'], "i")
        .replace('$', "s");
    let normalized_keywords = [
        "password",
        "token",
        "secret",
        "api_key",
        "apikey",
        "api-key",
        "authorization",
        "auth",
        "credential",
        "private key",
        "passphrase",
        "pin",
    ];
    normalized_keywords
        .iter()
        .any(|&kw| normalized.contains(kw))
}

/// Summarize a prompt into a short title using the configured LLM.
#[tauri::command]
pub async fn summarize_agent_title(
    state: State<'_, AppState>,
    raw_prompt: String,
) -> Result<String, String> {
    if prompt_is_sensitive(&raw_prompt) {
        return Ok("Sensitive prompt".to_string());
    }

    let orchestrator = Arc::clone(&state.orchestrator);
    match build_provider_config_from_store(&state) {
        Ok(config) => orchestrator.set_provider_config(config),
        Err(ProviderConfigError::MissingApiKey) => {
            return Err("no api key configured".to_string());
        }
    }
    orchestrator
        .summarize_title(&raw_prompt)
        .await
        .map(|t| t.trim().to_string())
        .map_err(|e| provider_chat_error(&state, e))
}

/// Clear all conversation history from the orchestrator.
#[tauri::command]
pub async fn athena_clear_context(state: State<'_, AppState>) -> Result<(), String> {
    let orchestrator = Arc::clone(&state.orchestrator);
    orchestrator.clear_context().await;
    Ok(())
}

/// Set the conversation history from a list of session entries.
#[tauri::command]
pub async fn athena_set_session_context(
    state: State<'_, AppState>,
    history: String,
) -> Result<(), String> {
    let entries: Vec<athena_core::types::SessionHistoryEntry> =
        serde_json::from_str(&history).map_err(|e| e.to_string())?;
    let orchestrator = Arc::clone(&state.orchestrator);
    orchestrator.set_session_context(entries).await;
    Ok(())
}

/// Provide an answer to a pending user question from the orchestrator.
#[tauri::command]
pub fn athena_user_answer(
    state: State<'_, AppState>,
    request_id: String,
    answer: String,
) -> Result<bool, String> {
    let mut map = state.pending_questions.lock();
    if let Some(tx) = map.remove(&request_id) {
        let _ = tx.send(answer);
        Ok(true)
    } else {
        log::warn!("no pending question found for request_id: {}", request_id);
        Ok(false)
    }
}

/// Store an API key securely in the OS keychain.
#[tauri::command]
pub fn store_api_key(key: String) -> Result<(), String> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .set_password(&key)
        .map_err(|e| format!("Failed to store API key in keyring: {}", e))
}

/// Clear the API key from the OS keychain.
#[tauri::command]
pub fn clear_api_key() -> Result<(), String> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .delete_credential()
        .or_else(|e| {
            if matches!(e, keyring::Error::NoEntry) {
                Ok(())
            } else {
                Err(e)
            }
        })
        .map_err(|e| format!("Failed to clear API key from keyring: {}", e))
}
