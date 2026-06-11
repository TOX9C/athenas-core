use serde::{Deserialize, Serialize};

/// Reference to an attached image in a session message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageRef {
    #[serde(rename = "imageId")]
    pub image_id: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    /// Optional human-readable name for the image.
    pub name: Option<String>,
}

/// Enum representing the possible roles in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "athena")]
    Athena,
}

/// A single message within a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMessage {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "role")]
    pub role: MessageRole,
    #[serde(rename = "content")]
    pub content: String,
    /// Timestamp as milliseconds since Unix epoch.
    #[serde(rename = "timestamp")]
    pub timestamp: u64,
    /// Whether this message represents an error condition.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// References to images attached to this message.
    #[serde(rename = "imageRefs", skip_serializing_if = "Option::is_none")]
    pub image_refs: Option<Vec<ImageRef>>,
}

/// Represents a chat session with messages and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "title")]
    pub title: String,
    /// Timestamp as milliseconds since Unix epoch.
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    /// Timestamp as milliseconds since Unix epoch.
    #[serde(rename = "updatedAt")]
    pub updated_at: u64,
    #[serde(rename = "messages")]
    pub messages: Vec<SessionMessage>,
}

/// Maximum number of messages retained in a single chat session.
///
/// Older non-system messages are evicted (FIFO) once this cap is reached so
/// that long-running sessions don't grow unbounded in memory or on disk.
pub const MAX_SESSION_MESSAGES: usize = 10_000;

/// Returns true if the message should be treated as a system message and
/// preserved across eviction. Today `MessageRole` only has `User` and
/// `Athena`; system prompts are stored separately in the orchestrator, so
/// this currently never matches. Kept as a hook so that adding a `System`
/// role later preserves eviction correctness.
fn is_system_message(_m: &SessionMessage) -> bool {
    // Matches the `m.role == "system"` style predicate from the spec.
    // Extend when MessageRole gains a System variant.
    false
}

impl ChatSession {
    /// Append a message, evicting the oldest messages (FIFO) if the total
    /// would exceed `MAX_SESSION_MESSAGES`. The newest message is always
    /// retained. System messages are preserved if `is_system_message` returns
    /// `true`; otherwise, the oldest entry is dropped to satisfy the cap.
    pub fn add_message_evicting(&mut self, msg: SessionMessage) {
        self.messages.push(msg);
        while self.messages.len() > MAX_SESSION_MESSAGES {
            // Find the oldest entry that is NOT a system message and drop it.
            // If every entry is a system message, fall back to dropping the
            // oldest entry (FIFO) to satisfy the cap.
            let drop_idx = self
                .messages
                .iter()
                .position(|m| !is_system_message(m))
                .unwrap_or(0);
            self.messages.remove(drop_idx);
        }
    }

    /// Truncate the messages vector to `MAX_SESSION_MESSAGES`, keeping the
    /// most recent messages. Call this after bulk updates (e.g. loading
    /// from disk or replacing the entire list) so a session loaded from a
    /// file larger than the cap is brought back under the limit.
    pub fn truncate_messages(&mut self) {
        let len = self.messages.len();
        if len > MAX_SESSION_MESSAGES {
            self.messages.drain(..len - MAX_SESSION_MESSAGES);
        }
    }
}

/// Metadata about an image stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageAttachment {
    #[serde(rename = "id")]
    pub id: String,
    /// Base64-encoded image data.
    #[serde(rename = "base64")]
    pub base64: String,
    /// MIME type of the image.
    #[serde(rename = "mediaType")]
    pub media_type: String,
    /// Optional human-readable name for the image.
    #[serde(rename = "name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Lightweight summary of a session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
    pub last_message_preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, role: MessageRole) -> SessionMessage {
        SessionMessage {
            id: id.to_string(),
            role,
            content: format!("content-{id}"),
            timestamp: 0,
            is_error: None,
            image_refs: None,
        }
    }

    #[test]
    fn add_message_evicting_caps_at_max() {
        let mut session = ChatSession {
            id: "s1".to_string(),
            title: "t".to_string(),
            created_at: 0,
            updated_at: 0,
            messages: Vec::new(),
        };

        // 1 "system" + 9999 user messages = 10_000 (at the cap, no eviction yet)
        session.add_message_evicting(make_msg("sys-0", MessageRole::User));
        for i in 0..9_999 {
            session.add_message_evicting(make_msg(&format!("u-{i:05}"), MessageRole::User));
        }
        assert_eq!(session.messages.len(), MAX_SESSION_MESSAGES);

        // Add 2 more — should evict 2 oldest messages. (MessageRole has no
        // System variant today; the "sys-0" message above is just a
        // placeholder so the test mirrors the spec. Once a System role
        // is added and `is_system_message` updated, the same drain logic
        // will preserve it.)
        session.add_message_evicting(make_msg("new-1", MessageRole::User));
        session.add_message_evicting(make_msg("new-2", MessageRole::User));

        assert_eq!(session.messages.len(), MAX_SESSION_MESSAGES);

        // The two oldest messages are gone. Since `is_system_message` always
        // returns false (no System role in MessageRole), the first two
        // messages inserted — sys-0 and u-00000 — are evicted.
        assert!(!session.messages.iter().any(|m| m.id == "sys-0"));
        assert!(!session.messages.iter().any(|m| m.id == "u-00000"));
        // The next-oldest survives because we only added 2 more messages.
        assert!(session.messages.iter().any(|m| m.id == "u-00001"));

        // The two newest messages are at the tail.
        assert_eq!(session.messages.last().unwrap().id, "new-2");
        assert_eq!(
            session.messages[session.messages.len() - 2].id,
            "new-1"
        );
    }

    #[test]
    fn truncate_messages_brings_oversized_session_under_cap() {
        let mut session = ChatSession {
            id: "s2".to_string(),
            title: "t".to_string(),
            created_at: 0,
            updated_at: 0,
            messages: (0..(MAX_SESSION_MESSAGES + 50))
                .map(|i| make_msg(&format!("m-{i:05}"), MessageRole::User))
                .collect(),
        };
        assert!(session.messages.len() > MAX_SESSION_MESSAGES);
        session.truncate_messages();
        assert_eq!(session.messages.len(), MAX_SESSION_MESSAGES);
        // Most recent messages are kept.
        assert_eq!(session.messages.last().unwrap().id, "m-10049");
    }
}
