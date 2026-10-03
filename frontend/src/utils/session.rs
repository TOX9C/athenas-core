use crate::tauri_bridge;
use dioxus::prelude::*;

// Single definitions live elsewhere; re-exported here where session-list
// callers already import them from.
pub use crate::stores::session::SessionListItem;
pub use crate::utils::time::{format_time_ago, format_time_ago_at};

/// Parse the backend's camelCase session-list payload.
///
/// Individual malformed entries are skipped so one corrupt record does not
/// hide otherwise usable sessions. A malformed top-level payload is reported
/// to the caller so the UI can preserve useful diagnostics.
pub fn parse_session_list(json: &str) -> Result<Vec<SessionListItem>, String> {
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|error| format!("invalid session list: {error}"))?;

    Ok(parsed
        .iter()
        .filter_map(|value| {
            Some(SessionListItem {
                id: value.get("id")?.as_str()?.to_string(),
                title: value.get("title")?.as_str()?.to_string(),
                // Older session payloads used by the switcher did not always
                // include creation time; keep them listable without inventing
                // a current timestamp.
                created_at: value
                    .get("createdAt")
                    .and_then(|created_at| created_at.as_i64())
                    .unwrap_or_default(),
                updated_at: value.get("updatedAt")?.as_i64()?,
                message_count: value.get("messageCount")?.as_u64()? as usize,
                last_message_preview: value
                    .get("lastMessagePreview")
                    .and_then(|preview| preview.as_str())
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect())
}

/// Load the session list from the backend.
pub async fn fetch_sessions() -> Result<Vec<SessionListItem>, String> {
    let json = tauri_bridge::session_list().await.map_err(|error| {
        error
            .as_string()
            .unwrap_or_else(|| format!("Tauri session_list error: {error:?}"))
    })?;
    parse_session_list(&json)
}

/// Load a session into the Athena store, replacing the visible conversation.
///
/// The ONE session-load path — used by the panel mount restore, the session
/// switcher, and any future entry point — so no variant can drift (the mount
/// restore previously skipped the cancel step and let late stream chunks from
/// the previous session land in the freshly loaded one).
///
/// Cancel semantics: the previous in-flight request is cancelled FIRST and
/// the active request is invalidated before the network fetch, closing the
/// race where a late chunk from the old conversation lands after the switch.
pub async fn load_session_into_store(
    session_id: &str,
    athena_state: &mut Signal<crate::stores::athena::AthenaState>,
) -> Result<(), String> {
    // Cancel the previous turn before replacing the visible conversation.
    let previous_request = athena_state.read().active_request_id.clone();
    if let Some(request_id) = previous_request {
        let _ = tauri_bridge::athena_cancel_stream(&request_id).await;
        athena_state.write().invalidate_active_request();
    }

    let json = tauri_bridge::session_get(session_id)
        .await
        .map_err(|error| format!("Failed to load session: {error:?}"))?;
    let val: serde_json::Value =
        serde_json::from_str(&json).map_err(|_| "Failed to parse session data".to_string())?;
    let messages = val
        .get("messages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "No messages field in session data".to_string())?;

    let loaded: Vec<crate::stores::athena::AthenaMessage> = messages
        .iter()
        .filter_map(|m| {
            use crate::stores::athena::MessageRole;
            let role_str = m.get("role")?.as_str()?;
            let role = if role_str.eq("user") {
                MessageRole::User
            } else {
                MessageRole::Athena
            };
            let content = m.get("content")?.as_str()?.to_string();
            let id = m
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let timestamp = m
                .get("timestamp")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64)
                as i64;
            let is_error = m.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
            Some(crate::stores::athena::AthenaMessage {
                id,
                role,
                content,
                timestamp,
                is_error,
                images: Vec::new(),
                usage: None,
                blocks: Vec::new(),
            })
        })
        .collect();

    let title = val
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("New Chat")
        .to_string();

    athena_state.write().set_messages(loaded);
    athena_state
        .write()
        .set_session_id(Some(session_id.to_string()));
    athena_state.write().set_session_title(title);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_list_skips_invalid_entries() {
        let json = r#"[
            {"id":"s1","title":"First","createdAt":100,"updatedAt":200,"messageCount":2,"lastMessagePreview":"hello"},
            {"id":"broken","title":"Missing updated time"},
            {"id":"s2","title":"Second","updatedAt":400,"messageCount":0}
        ]"#;

        let sessions = parse_session_list(json).expect("valid payload");

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "s1");
        assert_eq!(sessions[0].last_message_preview, "hello");
        assert_eq!(sessions[1].id, "s2");
        assert_eq!(sessions[1].created_at, 0);
        assert!(sessions[1].last_message_preview.is_empty());
    }

    #[test]
    fn parse_session_list_reports_invalid_top_level_json() {
        let error = parse_session_list("not-json").expect_err("invalid JSON should fail");

        assert!(error.contains("invalid session list"));
    }
}
