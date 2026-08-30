//! Conversation context and session persistence for the orchestrator.

use super::{build_anthropic_content, build_openai_content};
use super::{AnthropicMessage, AthenaOrchestrator, OpenAIMessage, SessionHistoryEntry};
use super::{OrchestratorError, StoreMessage, StoreMessageRole};
use std::sync::Arc;

/// Convert the in-memory OpenAI message buffer into store messages. Shared
/// by the synchronous save path and the debounced auto-save task (which owns
/// a snapshot instead of borrowing the orchestrator).
fn snapshot_store_messages(openai: &[OpenAIMessage]) -> Vec<StoreMessage> {
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
}

/// Persist a pre-taken message snapshot to `session_id`. Runs inside the
/// debounced auto-save task.
async fn save_snapshot(
    store: &athena_store::SessionStore,
    session_id: &str,
    snapshot: Vec<StoreMessage>,
) -> Result<(), OrchestratorError> {
    store
        .update_session(session_id, None, Some(snapshot))
        .await
        .map(|_| ())
        .map_err(|e| OrchestratorError::Generic(e.to_string()))
}

impl AthenaOrchestrator {
    /// Set the conversation history from a list of session entries.
    pub async fn set_session_context(&self, history: Vec<SessionHistoryEntry>) {
        // A context swap must not orphan the previous session's pending
        // debounced save: flush it synchronously first.
        if let Err(e) = self.flush_auto_save().await {
            log::warn!("flush before set_session_context: {}", e);
        }
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

    /// Clear all stored conversation context. Flushes any pending debounced
    /// save first so the outgoing session's final state is persisted before
    /// the buffers are wiped.
    pub async fn clear_context(&self) {
        if let Err(e) = self.flush_auto_save().await {
            log::warn!("flush before clear_context: {}", e);
        }
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

        let store_messages = snapshot_store_messages(&self.openai_messages.lock());

        store
            .update_session(session_id, None, Some(store_messages))
            .await
            .map_err(|e| OrchestratorError::Generic(e.to_string()))?;
        Ok(())
    }

    /// Load a previous conversation from the session store into the orchestrator.
    pub async fn load_conversation(&self, session_id: &str) -> Result<(), OrchestratorError> {
        // Switching sessions: flush the previous session's pending debounced
        // save before its buffers are replaced below.
        if let Err(e) = self.flush_auto_save().await {
            log::warn!("flush before load_conversation: {}", e);
        }
        let Some(store) = &self.session_store else {
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

    /// Schedule a debounced auto-save of the current conversation.
    ///
    /// Each completed turn calls this; the write fires ~2 s later unless a
    /// newer turn re-schedules first (then the pending task is aborted and
    /// the window restarts). Rapid consecutive turns thus coalesce into one
    /// full-file rewrite instead of one per turn. Crash window grows by the
    /// debounce interval — acceptable for chat history; `write_file_durable`
    /// atomic rename keeps the file itself consistent.
    pub fn try_auto_save(&self) {
        // The spawned task cannot hold `&self` across the 2 s window and the
        // orchestrator has no back-reference to its own Arc, so the task
        // owns only the data it needs: the store handle, the session id, and
        // a snapshot of the in-memory buffers taken under their locks (cheap
        // clone of two small Vecs vs. a full serialize+fsync per turn).
        // If the orchestrator drops before the timer fires, the save still
        // runs — persisting the last known state is correct.
        let Some(session_id) = self.get_current_session_id() else {
            return;
        };
        let Some(session_store) = self.session_store.clone() else {
            return;
        };
        let snapshot = snapshot_store_messages(&self.openai_messages.lock());
        let mut pending = self.auto_save_task.lock();
        match pending.take() {
            // Same session: a newer snapshot supersedes the pending write —
            // abort and reschedule.
            Some((sid, handle)) if sid == session_id => {
                handle.abort();
            }
            // Different session: the pending task's snapshot is
            // self-contained; leave it running detached so the previous
            // session's final state is still persisted after a fast switch.
            Some((_sid, handle)) => {
                drop(handle);
            }
            None => {}
        }
        *pending = Some((
            session_id.clone(),
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if let Err(e) = save_snapshot(&session_store, &session_id, snapshot).await {
                    log::warn!("Failed to auto-save conversation: {}", e);
                }
            }),
        ));
    }

    /// Flush the pending debounced auto-save immediately (if any) and wait
    /// for it. Called from the legacy (non-stream) turn path and before any
    /// context swap (`set_session_context`, `clear_context`,
    /// `load_conversation`) so a session switch cannot silently drop the
    /// previous session's pending write.
    pub async fn flush_auto_save(&self) -> Result<(), OrchestratorError> {
        let Some((_sid, handle)) = self.auto_save_task.lock().take() else {
            return Ok(());
        };
        match handle.await {
            Ok(()) => Ok(()),
            Err(e) if e.is_cancelled() => Ok(()),
            Err(e) => Err(OrchestratorError::Generic(format!(
                "auto-save task panicked: {e}"
            ))),
        }
    }
}

#[cfg(test)]
mod auto_save_tests {
    use super::*;
    use crate::orchestrator::AthenaOrchestrator;

    #[tokio::test]
    async fn flush_without_pending_save_is_noop() {
        let orch = AthenaOrchestrator::new();
        assert!(orch.flush_auto_save().await.is_ok());
    }

    #[tokio::test]
    async fn debounced_save_writes_after_window() {
        let mut orch = AthenaOrchestrator::new();
        let store = Arc::new(athena_store::SessionStore::new_empty());
        orch.set_session_store(Arc::clone(&store));
        let orch = Arc::new(orch);
        let session = store.create_session(Some("t")).await.expect("create");

        orch.set_current_session_id(session.id.clone());
        orch.openai_messages.lock().push(OpenAIMessage {
            role: "user".to_string(),
            content: serde_json::Value::String("hello".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        orch.try_auto_save();
        // Not yet persisted (within the 2 s window).
        let before = store.get_session(&session.id).await.expect("get").unwrap();
        assert_eq!(before.messages.len(), 0);
        // Flushing runs the pending task to completion.
        orch.flush_auto_save().await.expect("flush");
        let after = store.get_session(&session.id).await.expect("get").unwrap();
        assert_eq!(after.messages.len(), 1);
        assert_eq!(after.messages[0].content, "hello");
    }

    #[tokio::test]
    async fn same_session_reschedule_supersedes() {
        let mut orch = AthenaOrchestrator::new();
        let store = Arc::new(athena_store::SessionStore::new_empty());
        orch.set_session_store(Arc::clone(&store));
        let orch = Arc::new(orch);
        let session = store.create_session(Some("t")).await.expect("create");
        orch.set_current_session_id(session.id.clone());
        orch.openai_messages.lock().push(OpenAIMessage {
            role: "user".to_string(),
            content: serde_json::Value::String("first".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        orch.try_auto_save();
        orch.openai_messages.lock().push(OpenAIMessage {
            role: "assistant".to_string(),
            content: serde_json::Value::String("reply".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        orch.try_auto_save();
        orch.flush_auto_save().await.expect("flush");
        let after = store.get_session(&session.id).await.expect("get").unwrap();
        // The superseded 1-message write must NOT have landed; the final
        // 2-message snapshot must.
        assert_eq!(after.messages.len(), 2);
    }
}
