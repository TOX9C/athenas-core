use super::{caps, CommandError};
use crate::state::AppState;
use tauri::State;

/// Create a new chat session and return its JSON representation.
#[tauri::command]
pub async fn session_create(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<String, String> {
    if let Some(ref t) = title {
        caps::validate_title(t)?;
    }
    let session = state
        .session_store
        .create_session(title.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&session).map_err(|e| e.to_string())
}

/// Get a chat session by its ID.
#[tauri::command]
pub async fn session_get(state: State<'_, AppState>, id: String) -> Result<String, CommandError> {
    let session = state
        .session_store
        .get_session(&id)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match session {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
        None => Err(CommandError::NotFound(format!(
            "Session '{}' not found",
            id
        ))),
    }
}

/// List all chat sessions with summary information (id, title, message count, etc.).
#[tauri::command]
pub async fn session_list(state: State<'_, AppState>) -> Result<String, String> {
    let sessions = state
        .session_store
        .list_sessions()
        .await
        .map_err(|e| e.to_string())?;
    let mut json = Vec::new();
    for item in &sessions {
        json.push(serde_json::json!({
            "id": item.id,
            "title": item.title,
            "createdAt": item.created_at,
            "updatedAt": item.updated_at,
            "messageCount": item.message_count,
            "lastMessagePreview": item.last_message_preview
        }));
    }
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

/// Delete a chat session by its ID.
#[tauri::command]
pub async fn session_delete(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .session_store
        .delete_session(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("deleted".to_string())
}

/// Update a chat session's title and/or messages.
#[tauri::command]
pub async fn session_update(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    messages: Option<String>,
) -> Result<String, CommandError> {
    if let Some(ref t) = title {
        caps::validate_title(t).map_err(CommandError::InvalidInput)?;
    }
    let parsed_messages: Option<Vec<athena_store::SessionMessage>> = match messages {
        Some(json) => Some(
            serde_json::from_str(&json)
                .map_err(|e| CommandError::InvalidInput(format!("Invalid messages JSON: {}", e)))?,
        ),
        None => None,
    };
    let session = state
        .session_store
        .update_session(&id, title.as_deref(), parsed_messages)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    match session {
        Some(s) => serde_json::to_string(&s).map_err(|e| CommandError::Internal(e.to_string())),
        None => Err(CommandError::NotFound(format!(
            "Session '{}' not found",
            id
        ))),
    }
}

/// Add a message to an existing chat session.
#[tauri::command]
pub async fn session_add_message(
    state: State<'_, AppState>,
    session_id: String,
    role: String,
    content: String,
    is_error: Option<bool>,
    image_refs: Option<String>,
) -> Result<String, String> {
    let mut session = state
        .session_store
        .get_session(&session_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Session not found".to_string())?;

    let parsed_refs: Option<Vec<athena_store::ImageRef>> = match image_refs {
        Some(json) => Some(serde_json::from_str(&json).map_err(|e| e.to_string())?),
        None => None,
    };

    let message_role = match role.as_str() {
        "user" => athena_store::MessageRole::User,
        _ => athena_store::MessageRole::Athena,
    };

    let msg = athena_store::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: message_role,
        content,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        is_error,
        image_refs: parsed_refs,
    };

    session.messages.push(msg);
    let updated = state
        .session_store
        .update_session(&session_id, None, Some(session.messages))
        .await
        .map_err(|e| e.to_string())?;
    match updated {
        Some(s) => serde_json::to_string(&s).map_err(|e| e.to_string()),
        None => Err("Failed to update session".to_string()),
    }
}
