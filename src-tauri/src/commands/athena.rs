use super::CommandError;
use crate::state::AppState;
use tauri::State;

/// Build provider config from the persistent store for LLM API calls.
fn build_provider_config_from_store(state: &AppState) -> Option<athena_core::orchestrator::ProviderConfig> {
    let provider_str = state.store.get::<String>("llm.provider").ok().flatten().unwrap_or_else(|| "anthropic".to_string());
    let api_key = state.store.get::<String>("llm.api_key").ok().flatten()
        .or_else(|| {
            keyring::Entry::new("athena", "api_key")
                .ok()
                .and_then(|e| e.get_password().ok())
        })
        .unwrap_or_default();
    let model = state.store.get::<String>("llm.model").ok().flatten().unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    if api_key.is_empty() {
        log::warn!("No API key configured for LLM provider");
        return None;
    }

    let provider = match provider_str.as_str() {
        "anthropic" => athena_core::types::LLMProvider::Anthropic,
        "openai" => athena_core::types::LLMProvider::OpenAI,
        "nvidia_nim" => athena_core::types::LLMProvider::NvidiaNim,
        "lmstudio" => athena_core::types::LLMProvider::Lmstudio,
        _ => {
            log::warn!("Unknown LLM provider: {}", provider_str);
            return None;
        }
    };

    Some(athena_core::orchestrator::ProviderConfig {
        provider,
        api_key,
        model,
        system_prompt: String::new(),
        base_url: None,
    })
}

/// Send a text message to the configured LLM provider and return the response.
///
/// Holds the orchestrator lock for the entire set-config + send-message
/// sequence to prevent a TOCTOU race where another caller could change
/// the config between the set and send operations.
#[tauri::command]
pub async fn athena_chat(state: State<'_, AppState>, message: String) -> Result<String, CommandError> {
    let config = build_provider_config_from_store(&state);
    let orchestrator = state.orchestrator.lock().await;
    if let Some(cfg) = config {
        orchestrator.set_provider_config(cfg);
    }
    orchestrator.send_message(message, None).await.map_err(|e| CommandError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn athena_chat_with_session(
    state: State<'_, AppState>,
    message: String,
    session_id: String,
) -> Result<String, CommandError> {
    let config = build_provider_config_from_store(&state);
    let orchestrator = state.orchestrator.lock().await;
    if let Some(cfg) = config {
        orchestrator.set_provider_config(cfg);
    }
    orchestrator.set_current_session_id(session_id);
    orchestrator.send_message(message, None).await.map_err(|e| CommandError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn athena_chat_with_images(
    state: State<'_, AppState>,
    message: String,
    images: String,
) -> Result<String, CommandError> {
    let image_data: Vec<athena_core::types::ImageData> =
        serde_json::from_str(&images).map_err(|e| CommandError::InvalidInput(format!("Invalid images JSON: {}", e)))?;
    let config = build_provider_config_from_store(&state);
    let orchestrator = state.orchestrator.lock().await;
    if let Some(cfg) = config {
        orchestrator.set_provider_config(cfg);
    }
    orchestrator.send_message(message, Some(image_data)).await.map_err(|e| CommandError::Internal(e.to_string()))
}

/// Clear all conversation history from the orchestrator.
#[tauri::command]
pub async fn athena_clear_context(state: State<'_, AppState>) -> Result<(), CommandError> {
    let orchestrator = state.orchestrator.lock().await;
    orchestrator.clear_context();
    Ok(())
}

/// Set the conversation history from a list of session entries.
#[tauri::command]
pub async fn athena_set_session_context(
    state: State<'_, AppState>,
    history: String,
) -> Result<(), CommandError> {
    let entries: Vec<athena_core::types::SessionHistoryEntry> =
        serde_json::from_str(&history).map_err(|e| CommandError::InvalidInput(format!("Invalid history JSON: {}", e)))?;
    let orchestrator = state.orchestrator.lock().await;
    orchestrator.set_session_context(entries);
    Ok(())
}

/// Provide an answer to a pending user question from the orchestrator.
#[tauri::command]
pub fn athena_user_answer(
    state: State<'_, AppState>,
    request_id: String,
    answer: String,
) -> Result<bool, CommandError> {
    let mut map = state
        .pending_questions
        .lock()
        .map_err(|e| CommandError::Internal(format!("pending_questions lock poisoned: {}", e)))?;
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
pub fn store_api_key(key: String) -> Result<(), CommandError> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| CommandError::Internal(format!("Failed to create keyring entry: {}", e)))?;
    entry
        .set_password(&key)
        .map_err(|e| CommandError::Internal(format!("Failed to store API key in keyring: {}", e)))
}

/// Clear the API key from the OS keychain.
#[tauri::command]
pub fn clear_api_key() -> Result<(), CommandError> {
    let entry = keyring::Entry::new("athena", "api_key")
        .map_err(|e| CommandError::Internal(format!("Failed to create keyring entry: {}", e)))?;
    entry
        .delete_credential()
        .or_else(|e| {
            if matches!(e, keyring::Error::NoEntry) {
                Ok(())
            } else {
                Err(e)
            }
        })
        .map_err(|e| CommandError::Internal(format!("Failed to clear API key from keyring: {}", e)))
}
