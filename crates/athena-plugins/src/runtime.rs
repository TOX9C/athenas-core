//! Plugin sessions, event subscriptions, message relay, and health checks.

use std::collections::HashSet;
use std::time::Instant;

use super::{
    now_millis, scoped_capabilities, AgentType, HealthCheckResult, PendingMessage,
    PluginCapability, PluginError, PluginEvent, PluginEventPayload, PluginEventSource,
    PluginEventType, PluginManager, PluginSession, SessionStatus,
};

impl PluginManager {
    pub fn register_session(
        &self,
        plugin_id: impl Into<String>,
        agent_type: AgentType,
        agent_id: Option<String>,
        pane_id: Option<String>,
        requested_capabilities: Option<Vec<PluginCapability>>,
    ) -> Result<PluginSession, PluginError> {
        let id = uuid::Uuid::new_v4().to_string();
        let plugin_id = plugin_id.into();
        let agent_id = agent_id.unwrap_or_else(|| format!("agent-{}", &id[..8.min(id.len())]));
        let capabilities = scoped_capabilities(&agent_type, requested_capabilities);

        let now = now_millis();
        let session = PluginSession {
            id,
            plugin_id: plugin_id.clone(),
            agent_type,
            agent_id,
            pane_id,
            capabilities,
            connected_at: now,
            last_activity_at: now,
            status: SessionStatus::Active,
        };

        let mut inner = self.inner.lock()?;

        // Validate plugin exists
        if !inner.plugins.contains_key(&plugin_id) {
            return Err(PluginError::PluginNotFound(plugin_id));
        }

        inner.sessions.insert(session.id.clone(), session.clone());

        drop(inner);

        self.callbacks.on_session_registered(&session);

        Ok(session)
    }

    pub fn get_session(&self, session_id: &str) -> Option<PluginSession> {
        let inner = self.inner.lock().ok()?;
        inner.sessions.get(session_id).cloned()
    }

    pub fn get_session_by_agent_id(&self, agent_id: &str) -> Option<PluginSession> {
        let inner = self.inner.lock().ok()?;
        inner
            .sessions
            .values()
            .find(|s| s.agent_id == agent_id)
            .cloned()
    }

    pub fn list_sessions(&self) -> Vec<PluginSession> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner.sessions.values().cloned().collect()
    }

    pub fn remove_session(&self, session_id: &str) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let session = inner
            .sessions
            .get(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;

        let agent_id = session.agent_id.clone();

        // Remove event subscriptions for this session.
        for subscribers in inner.event_subscriptions.values_mut() {
            subscribers.remove(session_id);
        }

        // Remove any pending messages for this session.
        inner
            .pending_messages
            .retain(|_, msg| msg.session_id != session_id);

        inner.sessions.remove(session_id);

        drop(inner);

        self.callbacks.on_session_removed(session_id, &agent_id);

        Ok(())
    }

    pub fn update_session_status(
        &self,
        session_id: &str,
        status: SessionStatus,
        data: Option<&serde_json::Value>,
    ) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let session = inner
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;

        session.status = status;
        session.last_activity_at = now_millis();
        let agent_id = session.agent_id.clone();

        drop(inner);

        self.callbacks
            .on_session_status_update(session_id, &agent_id, status, data);

        Ok(())
    }

    pub fn subscribe_session(
        &self,
        session_id: &str,
        event_types: &[PluginEventType],
    ) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        // Verify the session exists.
        if !inner.sessions.contains_key(session_id) {
            return Err(PluginError::SessionNotFound(session_id.to_string()));
        }

        for event_type in event_types {
            inner
                .event_subscriptions
                .entry(event_type.clone())
                .or_insert_with(HashSet::new)
                .insert(session_id.to_string());
        }

        Ok(())
    }

    pub fn get_subscribers(&self, event_type: &PluginEventType) -> Vec<PluginSession> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        let subscriber_ids = match inner.event_subscriptions.get(event_type) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        subscriber_ids
            .iter()
            .filter_map(|id| {
                inner.sessions.get(id).and_then(|s| {
                    if s.status != SessionStatus::Disconnected {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    pub fn emit_plugin_event(
        &self,
        event_type: PluginEventType,
        source: PluginEventSource,
        payload: PluginEventPayload,
    ) -> PluginEvent {
        let event = PluginEvent {
            id: format!("evt-{}", &uuid::Uuid::new_v4().to_string()[..12]),
            event_type,
            source,
            payload,
            timestamp: now_millis(),
        };

        self.callbacks.on_plugin_event(&event);

        self.emit_event(
            "plugin:event",
            &serde_json::json!({
                "id": event.id,
                "type": event.event_type,
                "source": event.source,
                "payload": event.payload,
                "timestamp": event.timestamp,
            }),
        );

        event
    }

    pub fn send_message(
        &self,
        session_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<PendingMessage, PluginError> {
        let mut inner = self.inner.lock()?;

        let session = inner
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;

        if session.status == SessionStatus::Disconnected {
            return Err(PluginError::SessionNotFound(session_id.to_string()));
        }

        session.last_activity_at = now_millis();

        let msg_id = format!("msg-{}", uuid::Uuid::new_v4());
        let pending = PendingMessage {
            id: msg_id,
            session_id: session_id.to_string(),
            method: method.to_string(),
            params,
            sent_at: now_millis(),
        };

        inner
            .pending_messages
            .insert(pending.id.clone(), pending.clone());

        Ok(pending)
    }

    pub fn complete_message(&self, message_id: &str) -> Option<PendingMessage> {
        let mut inner = self.inner.lock().ok()?;
        inner.pending_messages.remove(message_id)
    }

    pub fn get_pending_messages(&self, session_id: &str) -> Vec<PendingMessage> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner
            .pending_messages
            .values()
            .filter(|m| m.session_id == session_id)
            .cloned()
            .collect()
    }

    pub fn health_check(&self) -> Result<HealthCheckResult, PluginError> {
        let mut inner = self.inner.lock()?;

        let now = now_millis();
        let stall_timeout_ms = inner.stall_timeout.as_millis() as i64;
        let mut stalled_ids = Vec::new();

        let mut active = 0usize;
        let mut idle = 0usize;
        let mut stalled = 0usize;
        let mut disconnected = 0usize;

        for session in inner.sessions.values_mut() {
            match session.status {
                SessionStatus::Disconnected => {
                    disconnected += 1;
                }
                SessionStatus::WaitingInput => {
                    active += 1;
                }
                SessionStatus::Active | SessionStatus::Idle => {
                    let elapsed = now - session.last_activity_at;
                    if elapsed > stall_timeout_ms {
                        session.status = SessionStatus::Idle;
                        stalled += 1;
                        stalled_ids.push(session.id.clone());
                    } else if session.status == SessionStatus::Active {
                        active += 1;
                    } else {
                        idle += 1;
                    }
                }
            }
        }

        let total = inner.sessions.len();
        inner.last_health_check = Some(Instant::now());

        // Collect agent IDs for stalled sessions before releasing the lock.
        let updates: Vec<(String, String)> = inner
            .sessions
            .iter()
            .filter(|(id, _)| stalled_ids.contains(id))
            .map(|(_, s)| (s.id.clone(), s.agent_id.clone()))
            .collect();

        drop(inner);

        // Emit status updates for stalled sessions outside the lock.
        for (session_id, agent_id) in updates {
            let data = serde_json::json!({ "reason": "stalled" });
            self.callbacks.on_session_status_update(
                &session_id,
                &agent_id,
                SessionStatus::Idle,
                Some(&data),
            );
        }

        Ok(HealthCheckResult {
            checked_at: now,
            total_sessions: total,
            active_sessions: active,
            idle_sessions: idle,
            stalled_sessions: stalled,
            disconnected_sessions: disconnected,
            stalled_session_ids: stalled_ids,
        })
    }

    pub fn last_health_check(&self) -> Option<Instant> {
        let inner = self.inner.lock().ok()?;
        inner.last_health_check
    }
}
