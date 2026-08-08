use crate::tauri_bridge;

/// A session summary returned by the backend list endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionListItem {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
    pub last_message_preview: String,
}

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
                    .and_then(|created_at| created_at.as_u64())
                    .unwrap_or_default(),
                updated_at: value.get("updatedAt")?.as_u64()?,
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

/// Format a Unix timestamp in milliseconds relative to a supplied current time.
pub fn format_time_ago_at(timestamp_ms: u64, now_ms: u64) -> String {
    let diff = now_ms.saturating_sub(timestamp_ms);
    let minutes = diff / 60_000;
    let hours = diff / 3_600_000;
    let days = diff / 86_400_000;

    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if minutes > 0 {
        format!("{}m ago", minutes)
    } else {
        "just now".to_string()
    }
}

/// Format a backend Unix-millisecond timestamp for display.
pub fn format_time_ago(timestamp_ms: u64) -> String {
    format_time_ago_at(timestamp_ms, crate::utils::time::now_ms())
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

    #[test]
    fn format_time_ago_uses_millisecond_timestamps() {
        let now = 10 * 86_400_000;

        assert_eq!(format_time_ago_at(now - 2 * 60_000, now), "2m ago");
        assert_eq!(format_time_ago_at(now - 3 * 3_600_000, now), "3h ago");
        assert_eq!(format_time_ago_at(now - 4 * 86_400_000, now), "4d ago");
        assert_eq!(format_time_ago_at(now + 1_000, now), "just now");
    }
}
