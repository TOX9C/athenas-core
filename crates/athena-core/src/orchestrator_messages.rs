//! Provider message DTOs used by the orchestrator and session persistence.

/// Internal representation of a message for Anthropic's API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct AnthropicMessage {
    pub(super) role: String,
    pub(super) content: serde_json::Value,
}

/// Internal representation of a message for OpenAI-compatible APIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct OpenAIMessage {
    pub(super) role: String,
    pub(super) content: serde_json::Value,
    /// OpenAI requires `tool_calls` at the top level of the assistant message,
    /// and `tool_call_id` on the tool-response message. We store them here so
    /// we can serialize correctly without ad-hoc JSON patching.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
}
