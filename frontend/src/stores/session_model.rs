//! Session list data contract.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Summary of a session for the session list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionListItem {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub last_message_preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_session_item_is_empty() {
        assert_eq!(SessionListItem::default().id, "");
        assert_eq!(SessionListItem::default().message_count, 0);
        assert!(SessionListItem::default().last_message_preview.is_empty());
    }

    #[test]
    fn session_item_preserves_summary_fields() {
        let item = SessionListItem {
            id: "session-1".to_string(),
            title: "Refactor stores".to_string(),
            created_at: 10,
            updated_at: 20,
            message_count: 3,
            last_message_preview: "Working".to_string(),
        };
        assert_eq!(item.id, "session-1");
        assert_eq!(item.title, "Refactor stores");
        assert_eq!(item.updated_at, 20);
        assert_eq!(item.message_count, 3);
    }
}
