//! Model types for the per-agent message queue.

/// One queued message awaiting delivery to a busy agent.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedMessage {
    pub id: String,
    pub text: String,
}
