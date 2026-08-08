//! Conversation context and session persistence for the orchestrator.

use super::{build_anthropic_content, build_openai_content};
use super::{AnthropicMessage, AthenaOrchestrator, OpenAIMessage, SessionHistoryEntry};
use super::{OrchestratorError, StoreMessage, StoreMessageRole};
use std::sync::Arc;

impl AthenaOrchestrator {
    /// Set the conversation history from a list of session entries.
    pub fn set_session_context(&self, history: Vec<SessionHistoryEntry>) {
        let anthropic: Vec<AnthropicMessage> = history
            .iter()
            .map(|entry| AnthropicMessage {
                role: entry.role.clone(),
                content: build_anthropic_content(&entry.content, entry.images.as_deref()),
            })
            .collect();

        let openai: Vec<OpenAIMessage> = history
            .iter()
            .map(|entry| OpenAIMessage {
                role: entry.role.clone(),
                content: build_openai_content(&entry.content, entry.images.as_deref()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();

        {
            let mut a = self.anthropic_messages.lock();
            *a = anthropic;
        }
        {
            let mut o = self.openai_messages.lock();
            *o = openai;
        }
    }

    /// Clear all stored conversation context.
    pub fn clear_context(&self) {
        self.anthropic_messages.lock().clear();
        self.openai_messages.lock().clear();
        *self.current_session_id.lock() = None;
    }

    /// Set the current session identifier.
    pub fn set_current_session_id(&self, id: String) {
        *self.current_session_id.lock() = Some(id);
    }

    /// Get the current session identifier, if any.
    pub fn get_current_session_id(&self) -> Option<String> {
        self.current_session_id.lock().clone()
    }

    /// Set the session store for persisting conversations.
    pub fn set_session_store(&mut self, store: Arc<athena_store::SessionStore>) {
        self.session_store = Some(store);
    }

    /// Get a reference to the session store, if configured.
    pub fn session_store(&self) -> Option<&Arc<athena_store::SessionStore>> {
        self.session_store.as_ref()
    }

    /// Save the current conversation history to the session store.
    pub async fn save_conversation(&self, session_id: &str) -> Result<(), OrchestratorError> {
        let Some(ref store) = self.session_store else {
            return Ok(());
        };

        // Prefer openai messages (default format) for persistence.
        let store_messages: Vec<StoreMessage> = {
            let openai = self.openai_messages.lock();

            let mut store_messages = Vec::new();
            for msg in openai.iter() {
                if msg.role == "system" || msg.role == "tool" {
                    continue;
                }
                let role = if msg.role == "user" {
                    StoreMessageRole::User
                } else {
                    StoreMessageRole::Athena
                };
                let content = match &msg.content {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                };
                store_messages.push(StoreMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role,
                    content,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    is_error: None,
                    image_refs: None,
                });
            }
            store_messages
        }; // guard dropped here before any await

        store
            .update_session(session_id, None, Some(store_messages))
            .await
            .map_err(|e| OrchestratorError::Generic(e.to_string()))?;
        Ok(())
    }

    /// Load a previous conversation from the session store into the orchestrator.
    pub async fn load_conversation(&self, session_id: &str) -> Result<(), OrchestratorError> {
        let Some(ref store) = self.session_store else {
            return Ok(());
        };

        let session = store
            .get_session(session_id)
            .await
            .map_err(|e| OrchestratorError::Generic(e.to_string()))?
            .ok_or_else(|| {
                OrchestratorError::Generic(format!("Session '{}' not found", session_id))
            })?;

        let anthropic: Vec<AnthropicMessage> = session
            .messages
            .iter()
            .map(|msg| AnthropicMessage {
                role: match msg.role {
                    StoreMessageRole::User => "user".to_string(),
                    StoreMessageRole::Athena => "assistant".to_string(),
                },
                content: serde_json::Value::String(msg.content.clone()),
            })
            .collect();

        let openai: Vec<OpenAIMessage> = session
            .messages
            .iter()
            .map(|msg| OpenAIMessage {
                role: match msg.role {
                    StoreMessageRole::User => "user".to_string(),
                    StoreMessageRole::Athena => "assistant".to_string(),
                },
                content: serde_json::Value::String(msg.content.clone()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();

        *self.anthropic_messages.lock() = anthropic;
        *self.openai_messages.lock() = openai;
        *self.current_session_id.lock() = Some(session_id.to_string());

        Ok(())
    }

    /// Attempt to auto-save the current conversation to the session store.
    pub async fn try_auto_save(&self) -> Result<(), OrchestratorError> {
        if let Some(session_id) = self.get_current_session_id() {
            self.save_conversation(&session_id).await
        } else {
            Ok(())
        }
    }
}
