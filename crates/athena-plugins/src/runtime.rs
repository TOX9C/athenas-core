//! Plugin sessions, event subscriptions, message relay, and health checks.

use std::collections::HashSet;
use std::time::Instant;

use super::{
    now_millis, scoped_capabilities_with_manifest, AgentType, HealthCheckResult, PendingMessage,
    PluginCapability, PluginError, PluginEvent, PluginEventPayload, PluginEventSource,
    PluginEventType, PluginManager, PluginManagerInner, PluginSession, SessionStatus,
    MAX_PENDING_PLUGIN_MESSAGES, MAX_PLUGIN_EVENT_BYTES, MAX_PLUGIN_SESSIONS,
    MAX_SESSION_SUBSCRIPTIONS, PENDING_PLUGIN_MESSAGE_TTL,
};

/// Drop pending messages older than the shared TTL. Uses `saturating_sub`
/// so a backwards clock shift keeps messages instead of panicking.
fn purge_expired_pending(inner: &mut PluginManagerInner, now: i64) {
    inner.pending_messages.retain(|_, message| {
        now.saturating_sub(message.sent_at) <= PENDING_PLUGIN_MESSAGE_TTL.as_millis() as i64
    });
}

/// Remove a pending message, returning it only when its session still exists.
fn remove_pending_message(
    inner: &mut PluginManagerInner,
    message_id: &str,
) -> Option<PendingMessage> {
    let message = inner.pending_messages.remove(message_id)?;
    if !inner.sessions.contains_key(&message.session_id) {
        return None;
    }
    Some(message)
}

/// Conservative (never undercounting) bound on the serialized size of a JSON
/// value, in bytes. Strings are bounded at 6x their UTF-8 length, which
/// covers the worst case where every character escapes as `\uXXXX`.
fn json_size_upper_bound(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Null | serde_json::Value::Bool(_) => 8,
        serde_json::Value::Number(_) => 32,
        serde_json::Value::String(s) => 2 + 6 * s.len(),
        serde_json::Value::Array(a) => 2 + a.iter().map(json_size_upper_bound).sum::<usize>(),
        serde_json::Value::Object(o) => {
            2 + o
                .iter()
                .map(|(k, val)| 3 + 6 * k.len() + json_size_upper_bound(val))
                .sum::<usize>()
        }
    }
}

/// Upper bound on the serialized [`PluginEventPayload`] size without paying
/// for an encoding pass. The exact check runs only when this crosses the cap.
fn payload_size_upper_bound(p: &PluginEventPayload) -> usize {
    // Structural slack: braces, field keys, colons, commas, and the
    // fixed-size `level`/`exit_code` scalars.
    let mut size = 256usize;
    for field in [
        &p.message,
        &p.title,
        &p.task_title,
        &p.result,
        &p.error,
        &p.prompt,
        &p.request_id,
        &p.response,
        &p.command,
        &p.session_id,
        &p.agent_id,
        &p.plugin_id,
    ] {
        size += field.as_ref().map_or(0, |s| 2 + 6 * s.len());
    }
    if let Some(options) = &p.options {
        size += 2 + options.iter().map(|o| 3 + 6 * o.len()).sum::<usize>();
    }
    if let Some(metadata) = &p.metadata {
        size += json_size_upper_bound(metadata);
    }
    size
}

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

        let now = now_millis();
        let session = PluginSession {
            id,
            plugin_id: plugin_id.clone(),
            agent_type,
            agent_id,
            pane_id,
            capabilities: Vec::new(),
            connected_at: now,
            last_activity_at: now,
            status: SessionStatus::Active,
        };

        let mut inner = self.inner.lock()?;

        // A disabled/error plugin cannot create new host sessions. Installed
        // plugins remain usable for compatibility with the existing host flow;
        // the explicit enable operation is optional for built-in integrations.
        let declared_capabilities = inner
            .plugins
            .get(&plugin_id)
            .map(|entry| entry.manifest.capabilities.clone());
        match inner.plugins.get(&plugin_id).map(|entry| entry.status) {
            None => return Err(PluginError::PluginNotFound(plugin_id)),
            Some(super::PluginStatus::Disabled | super::PluginStatus::Error) => {
                return Err(PluginError::ValidationFailed(
                    "plugin is disabled or in an error state".to_string(),
                ))
            }
            Some(_) => {}
        }
        if inner.sessions.len() >= MAX_PLUGIN_SESSIONS {
            return Err(PluginError::LimitExceeded(
                "maximum plugin session count reached".to_string(),
            ));
        }

        // Scope requested capabilities by both agent defaults and the plugin
        // manifest declaration. A plugin cannot gain capabilities by asking
        // for more than its manifest advertises.
        let mut session = session;
        session.capabilities = scoped_capabilities_with_manifest(
            &session.agent_type,
            requested_capabilities,
            declared_capabilities.as_deref(),
        );
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
        self.remove_session_impl(None, session_id)
    }

    /// Remove a session only when it belongs to `plugin_id`.
    ///
    /// This is the ownership-aware seam for authenticated plugin-host callers;
    /// the legacy ID-only method remains for trusted in-process cleanup paths.
    pub fn remove_session_owned(
        &self,
        plugin_id: &str,
        session_id: &str,
    ) -> Result<(), PluginError> {
        self.remove_session_impl(Some(plugin_id), session_id)
    }

    fn remove_session_impl(
        &self,
        owner: Option<&str>,
        session_id: &str,
    ) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let session = inner
            .sessions
            .get(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;
        if let Some(plugin_id) = owner {
            if session.plugin_id != plugin_id {
                return Err(PluginError::SessionOwnership {
                    session_id: session_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                });
            }
        }

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

    /// Subscribe a session only when it belongs to `plugin_id`.
    pub fn subscribe_session_owned(
        &self,
        plugin_id: &str,
        session_id: &str,
        event_types: &[PluginEventType],
    ) -> Result<(), PluginError> {
        self.subscribe_session_impl(Some(plugin_id), session_id, event_types)
    }

    fn subscribe_session_impl(
        &self,
        owner: Option<&str>,
        session_id: &str,
        event_types: &[PluginEventType],
    ) -> Result<(), PluginError> {
        if event_types.len() > MAX_SESSION_SUBSCRIPTIONS {
            return Err(PluginError::LimitExceeded(
                "too many event subscriptions".to_string(),
            ));
        }
        let mut inner = self.inner.lock()?;
        let session = inner
            .sessions
            .get(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;
        let owner_plugin_id = session.plugin_id.clone();
        if let Some(plugin_id) = owner {
            if session.plugin_id != plugin_id {
                return Err(PluginError::SessionOwnership {
                    session_id: session_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                });
            }
        }
        let plugin = inner
            .plugins
            .get(&owner_plugin_id)
            .ok_or_else(|| PluginError::PluginNotFound(owner_plugin_id.clone()))?;
        if matches!(
            plugin.status,
            super::PluginStatus::Disabled | super::PluginStatus::Error
        ) {
            return Err(PluginError::ValidationFailed(
                "plugin is disabled or in an error state".to_string(),
            ));
        }
        if let Some(declared) = &plugin.manifest.subscribes_to {
            if event_types.iter().any(|event| !declared.contains(event)) {
                return Err(PluginError::ValidationFailed(
                    "event subscription is not declared by the plugin".to_string(),
                ));
            }
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

    /// Queue a message only when the target session belongs to `plugin_id`.
    pub fn send_message_owned(
        &self,
        plugin_id: &str,
        session_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<PendingMessage, PluginError> {
        self.send_message_impl(Some(plugin_id), session_id, method, params)
    }

    fn send_message_impl(
        &self,
        owner: Option<&str>,
        session_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<PendingMessage, PluginError> {
        if method.len() > 256 {
            return Err(PluginError::LimitExceeded(
                "plugin message method is too long".to_string(),
            ));
        }
        let params_size = serde_json::to_vec(&params)
            .map(|bytes| bytes.len())
            .unwrap_or(usize::MAX);
        if params_size > MAX_PLUGIN_EVENT_BYTES {
            return Err(PluginError::LimitExceeded(
                "plugin message parameters exceed 256 KiB".to_string(),
            ));
        }
        let mut inner = self.inner.lock()?;
        let now = now_millis();
        purge_expired_pending(&mut inner, now);
        if inner.pending_messages.len() >= MAX_PENDING_PLUGIN_MESSAGES {
            return Err(PluginError::LimitExceeded(
                "maximum pending plugin message count reached".to_string(),
            ));
        }
        let session = inner
            .sessions
            .get(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;
        if let Some(plugin_id) = owner {
            if session.plugin_id != plugin_id {
                return Err(PluginError::SessionOwnership {
                    session_id: session_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                });
            }
        }
        let owner_plugin_id = session.plugin_id.clone();
        let plugin_status = inner
            .plugins
            .get(&owner_plugin_id)
            .map(|plugin| plugin.status)
            .ok_or_else(|| PluginError::PluginNotFound(owner_plugin_id.clone()))?;
        if matches!(
            plugin_status,
            super::PluginStatus::Disabled | super::PluginStatus::Error
        ) {
            return Err(PluginError::ValidationFailed(
                "plugin is disabled or in an error state".to_string(),
            ));
        }
        let session = inner
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PluginError::SessionNotFound(session_id.to_string()))?;
        if session.status == SessionStatus::Disconnected {
            return Err(PluginError::SessionNotFound(session_id.to_string()));
        }
        session.last_activity_at = now;
        let message = PendingMessage {
            id: format!("msg-{}", uuid::Uuid::new_v4()),
            session_id: session_id.to_string(),
            method: method.to_string(),
            params,
            sent_at: now,
        };
        inner
            .pending_messages
            .insert(message.id.clone(), message.clone());
        Ok(message)
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
        self.subscribe_session_impl(None, session_id, event_types)
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
    ) -> Result<PluginEvent, PluginError> {
        // Fast path: a conservative upper bound on the serialized payload
        // size avoids encoding every event twice (`to_vec` for the size
        // check, then again in the emit below). Only payloads whose bound
        // crosses the cap pay for the exact encoding.
        if payload_size_upper_bound(&payload) > MAX_PLUGIN_EVENT_BYTES
            && serde_json::to_vec(&payload)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX)
                > MAX_PLUGIN_EVENT_BYTES
        {
            log::warn!("[plugin-manager] rejecting oversized event payload (> {MAX_PLUGIN_EVENT_BYTES} bytes)");
            return Err(PluginError::LimitExceeded(
                "plugin event payload exceeds 256 KiB".to_string(),
            ));
        }
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

        Ok(event)
    }

    pub fn send_message(
        &self,
        session_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<PendingMessage, PluginError> {
        self.send_message_impl(None, session_id, method, params)
    }

    /// Consume a pending message by id (trusted in-process callers only; the
    /// id is not ownership-checked).
    pub fn complete_message(&self, message_id: &str) -> Option<PendingMessage> {
        let mut inner = self.inner.lock().ok()?;
        remove_pending_message(&mut inner, message_id)
    }

    /// Consume a pending message only when its session belongs to
    /// `plugin_id`. This is the ownership-aware seam for authenticated
    /// plugin-host callers.
    pub fn complete_message_owned(
        &self,
        plugin_id: &str,
        message_id: &str,
    ) -> Result<Option<PendingMessage>, PluginError> {
        let mut inner = self.inner.lock()?;
        if let Some(message) = inner.pending_messages.get(message_id) {
            if let Some(session) = inner.sessions.get(&message.session_id) {
                if session.plugin_id != plugin_id {
                    return Err(PluginError::SessionOwnership {
                        session_id: session.id.clone(),
                        plugin_id: plugin_id.to_string(),
                    });
                }
            }
        }
        Ok(remove_pending_message(&mut inner, message_id))
    }

    pub fn get_pending_messages(&self, session_id: &str) -> Vec<PendingMessage> {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        purge_expired_pending(&mut inner, now_millis());
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
        purge_expired_pending(&mut inner, now);
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
                    let elapsed = now.saturating_sub(session.last_activity_at);
                    if session.status == SessionStatus::Active && elapsed > stall_timeout_ms {
                        // Only notify on the Active -> Idle transition so an
                        // already-idle session is not re-reported every pass.
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
