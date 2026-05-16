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

/// Metadata about an image stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageAttachment {
    #[serde(rename = "id")]
    pub id: String,
    /// Base64-encoded image data.
    #[serde(rename = "base64")]
    pub base64: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
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
