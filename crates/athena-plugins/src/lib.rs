//! Plugin management crate for Athena's Core.
//!
//! Mirrors the Electron `pluginHost.ts` and `plugin-manager.ts` services,
//! providing plugin discovery, registration, session management, event relay,
//! and health checking as a pure data/coordination layer. Actual TCP/network
//! communication with external plugins is handled by the src-tauri commands
//! that call into this manager.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;

mod types;
pub use types::*;
mod validation;
pub use validation::{validate_plugin_install_method, validate_plugin_manifest};
#[path = "discovery.rs"]
mod discovery;
#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "runtime.rs"]
mod runtime;

use athena_core::EventEmitter;

/// Maximum size in bytes for a plugin manifest file. Manifests larger than
/// this are skipped during discovery to prevent a malicious or accidental
/// oversized file from exhausting memory.
pub const MAX_MANIFEST_BYTES: u64 = 1_048_576;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors produced by the plugin manager.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    PluginNotFound(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("plugin already registered and active: {0}")]
    AlreadyRegistered(String),

    #[error("manifest IO error: {0}")]
    ManifestIo(#[from] std::io::Error),

    #[error("manifest parse error in {path}: {source}")]
    ManifestParse {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("lock poisoned")]
    LockPoisoned,

    #[error("manifest validation failed: {0}")]
    ValidationFailed(String),
}

impl From<std::sync::PoisonError<std::sync::MutexGuard<'_, PluginManagerInner>>> for PluginError {
    fn from(_: std::sync::PoisonError<std::sync::MutexGuard<'_, PluginManagerInner>>) -> Self {
        PluginError::LockPoisoned
    }
}

// ---------------------------------------------------------------------------
// Callback trait (for Tauri commands to hook into)
// ---------------------------------------------------------------------------

/// Callback trait for plugin manager events. Implementations are provided by
/// the Tauri layer to emit events to the renderer and push notifications.
pub trait PluginCallbacks: Send + Sync {
    /// Called when a session is registered.
    fn on_session_registered(&self, session: &PluginSession);

    /// Called when a session is removed.
    fn on_session_removed(&self, session_id: &str, agent_id: &str);

    /// Called when a session status changes.
    fn on_session_status_update(
        &self,
        session_id: &str,
        agent_id: &str,
        status: SessionStatus,
        data: Option<&serde_json::Value>,
    );

    /// Called when a plugin event is emitted.
    fn on_plugin_event(&self, event: &PluginEvent);

    /// Called when the plugin registry changes.
    fn on_registry_updated(&self, registry: &HashMap<String, PluginInfo>);

    /// Called when a plugin is registered.
    fn on_plugin_registered(&self, plugin_id: &str, name: &str);

    /// Called when a plugin is enabled.
    fn on_plugin_enabled(&self, plugin_id: &str, name: &str);

    /// Called when a plugin is disabled.
    fn on_plugin_disabled(&self, plugin_id: &str);

    /// Called when a plugin encounters an error.
    fn on_plugin_error(&self, plugin_id: &str, error: &str);

    /// Called when plugin config changes.
    fn on_plugin_configured(&self, plugin_id: &str, config: &serde_json::Value);
}

/// A no-op callback implementation that does nothing. Useful for testing and
/// as a default when no callbacks are needed.
pub struct NoopCallbacks;

impl PluginCallbacks for NoopCallbacks {
    fn on_session_registered(&self, _session: &PluginSession) {}
    fn on_session_removed(&self, _session_id: &str, _agent_id: &str) {}
    fn on_session_status_update(
        &self,
        _session_id: &str,
        _agent_id: &str,
        _status: SessionStatus,
        _data: Option<&serde_json::Value>,
    ) {
    }
    fn on_plugin_event(&self, _event: &PluginEvent) {}
    fn on_registry_updated(&self, _registry: &HashMap<String, PluginInfo>) {}
    fn on_plugin_registered(&self, _plugin_id: &str, _name: &str) {}
    fn on_plugin_enabled(&self, _plugin_id: &str, _name: &str) {}
    fn on_plugin_disabled(&self, _plugin_id: &str) {}
    fn on_plugin_error(&self, _plugin_id: &str, _error: &str) {}
    fn on_plugin_configured(&self, _plugin_id: &str, _config: &serde_json::Value) {}
}

// ---------------------------------------------------------------------------
// Internal mutable state
// ---------------------------------------------------------------------------

pub(crate) struct PluginManagerInner {
    pub(crate) plugins: HashMap<String, PluginEntry>,
    pub(crate) sessions: HashMap<String, PluginSession>,
    pub(crate) event_subscriptions: HashMap<PluginEventType, HashSet<String>>,
    pub(crate) pending_messages: HashMap<String, PendingMessage>,
    pub(crate) stall_timeout: Duration,
    pub(crate) last_health_check: Option<Instant>,
}

// ---------------------------------------------------------------------------
// PluginManager
// ---------------------------------------------------------------------------

/// Thread-safe plugin manager. Wraps all mutable state in a single
/// `Arc<Mutex<PluginManagerInner>>` to avoid nested locking and deadlocks.
pub struct PluginManager {
    pub(crate) inner: Arc<Mutex<PluginManagerInner>>,
    pub(crate) callbacks: Arc<dyn PluginCallbacks>,
    pub(crate) event_emitter: EventEmitter,
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("inner", &"<Mutex<PluginManagerInner>>")
            .field("callbacks", &"<PluginCallbacks>")
            .field("event_emitter", &"<Option>")
            .finish()
    }
}

impl Clone for PluginManager {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            callbacks: Arc::clone(&self.callbacks),
            event_emitter: Arc::clone(&self.event_emitter),
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    // -- Construction -------------------------------------------------------

    /// Create a new plugin manager with no-op callbacks.
    pub fn new() -> Self {
        Self::with_callbacks(Arc::new(NoopCallbacks))
    }

    /// Create a new plugin manager with custom callbacks.
    pub fn with_callbacks(callbacks: Arc<dyn PluginCallbacks>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PluginManagerInner {
                plugins: HashMap::new(),
                sessions: HashMap::new(),
                event_subscriptions: HashMap::new(),
                pending_messages: HashMap::new(),
                stall_timeout: Duration::from_secs(90),
                last_health_check: None,
            })),
            callbacks,
            event_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    pub(crate) fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        if let Ok(guard) = self.event_emitter.lock() {
            if let Some(ref emitter) = *guard {
                emitter(channel, data);
                return;
            }
        }
        log::debug!("[plugin-manager] {} -> {}", channel, data);
    }

    /// Set the stall timeout for health checking. Sessions with no activity
    /// for longer than this duration are marked idle.
    pub fn set_stall_timeout(&self, timeout: Duration) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;
        inner.stall_timeout = timeout;
        Ok(())
    }
}

/// Build a `HashMap<String, PluginInfo>` from the current state while holding
/// the lock, so callers can release the lock before invoking callbacks.
pub(crate) fn build_registry_info(
    inner: &std::sync::MutexGuard<'_, PluginManagerInner>,
) -> HashMap<String, PluginInfo> {
    inner
        .plugins
        .values()
        .map(|e| {
            (
                e.manifest.id.clone(),
                PluginInfo {
                    id: e.manifest.id.clone(),
                    name: e.manifest.name.clone(),
                    version: e.manifest.version.clone(),
                    description: e.manifest.description.clone(),
                    author: e.manifest.author.clone(),
                    status: e.status,
                    config: e.config.clone(),
                    error: e.error.clone(),
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Standalone helper functions
// ---------------------------------------------------------------------------

/// Returns the default capabilities for a given agent type.
/// Mirrors `DEFAULT_CAPABILITIES` in the TS `plugin.ts`.
pub fn default_capabilities(agent_type: &AgentType) -> Vec<PluginCapability> {
    match agent_type {
        AgentType::Claude
        | AgentType::Codex
        | AgentType::Opencode
        | AgentType::Gemini
        | AgentType::Qwen
        | AgentType::Aider
        | AgentType::Cursor
        | AgentType::Freebuff
        | AgentType::Omp => vec![
            PluginCapability::Notifications,
            PluginCapability::Status,
            PluginCapability::Tasks,
            PluginCapability::UserInput,
        ],
        AgentType::Custom | AgentType::Shell => {
            vec![PluginCapability::Notifications, PluginCapability::Status]
        }
    }
}

/// Intersect requested capabilities with the defaults allowed for an agent
/// type. Mirrors `scopedCapabilities` in the TS `pluginHost.ts`.
pub(crate) fn scoped_capabilities(
    agent_type: &AgentType,
    requested: Option<Vec<PluginCapability>>,
) -> Vec<PluginCapability> {
    let allowed = default_capabilities(agent_type);
    match requested {
        Some(req) => {
            let allowed_set: HashSet<_> = allowed.into_iter().collect();
            req.into_iter()
                .filter(|cap| allowed_set.contains(cap))
                .collect()
        }
        None => allowed,
    }
}

/// Current time in milliseconds since Unix epoch.
pub(crate) fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A callback implementation that counts invocations for testing.
    struct CountingCallbacks {
        session_registered: AtomicUsize,
        session_removed: AtomicUsize,
        registry_updated: AtomicUsize,
        plugin_registered: AtomicUsize,
    }

    impl CountingCallbacks {
        fn new() -> Self {
            Self {
                session_registered: AtomicUsize::new(0),
                session_removed: AtomicUsize::new(0),
                registry_updated: AtomicUsize::new(0),
                plugin_registered: AtomicUsize::new(0),
            }
        }
    }

    impl PluginCallbacks for CountingCallbacks {
        fn on_session_registered(&self, _session: &PluginSession) {
            self.session_registered.fetch_add(1, Ordering::Relaxed);
        }
        fn on_session_removed(&self, _session_id: &str, _agent_id: &str) {
            self.session_removed.fetch_add(1, Ordering::Relaxed);
        }
        fn on_session_status_update(
            &self,
            _session_id: &str,
            _agent_id: &str,
            _status: SessionStatus,
            _data: Option<&serde_json::Value>,
        ) {
        }
        fn on_plugin_event(&self, _event: &PluginEvent) {}
        fn on_registry_updated(&self, _registry: &HashMap<String, PluginInfo>) {
            self.registry_updated.fetch_add(1, Ordering::Relaxed);
        }
        fn on_plugin_registered(&self, _plugin_id: &str, _name: &str) {
            self.plugin_registered.fetch_add(1, Ordering::Relaxed);
        }
        fn on_plugin_enabled(&self, _plugin_id: &str, _name: &str) {}
        fn on_plugin_disabled(&self, _plugin_id: &str) {}
        fn on_plugin_error(&self, _plugin_id: &str, _error: &str) {}
        fn on_plugin_configured(&self, _plugin_id: &str, _config: &serde_json::Value) {}
    }

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Test Plugin {id}"),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            permissions: vec![PluginCapability::Notifications],
            mcp_config: None,
            min_athena_version: None,
            capabilities: vec![PluginCapability::Notifications, PluginCapability::Status],
            tools: vec![],
            subscribes_to: None,
            config: None,
            install: None,
        }
    }

    // -- Plugin registration tests ------------------------------------------

    #[test]
    fn register_plugin_inserts_into_registry() {
        let mgr = PluginManager::new();
        let id = mgr.register_plugin(sample_manifest("p1")).unwrap();
        assert_eq!(id, "p1");
        assert_eq!(mgr.list_plugins().len(), 1);
    }

    #[test]
    fn register_plugin_rejects_duplicate_active() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();
        let result = mgr.register_plugin(sample_manifest("p1"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PluginError::AlreadyRegistered(_)
        ));
    }

    #[test]
    fn register_plugin_allows_reregister_after_disable() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();
        mgr.disable_plugin("p1").unwrap();
        let result = mgr.register_plugin(sample_manifest("p1"));
        assert!(result.is_ok());
    }

    #[test]
    fn unregister_plugin_removes_from_registry() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();
        mgr.unregister_plugin("p1").unwrap();
        assert!(mgr.list_plugins().is_empty());
    }

    #[test]
    fn unregister_plugin_errors_on_unknown() {
        let mgr = PluginManager::new();
        let result = mgr.unregister_plugin("nonexistent");
        assert!(result.is_err());
    }

    // -- Enable / disable / error tests -------------------------------------

    #[test]
    fn enable_disable_lifecycle() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        mgr.enable_plugin("p1").unwrap();
        let entry = mgr.get_plugin("p1").unwrap();
        assert_eq!(entry.status, PluginStatus::Enabled);

        mgr.disable_plugin("p1").unwrap();
        let entry = mgr.get_plugin("p1").unwrap();
        assert_eq!(entry.status, PluginStatus::Disabled);
    }

    #[test]
    fn enable_is_idempotent() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();
        mgr.enable_plugin("p1").unwrap();
        mgr.enable_plugin("p1").unwrap(); // no error
    }

    #[test]
    fn set_plugin_error_marks_status() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();
        mgr.set_plugin_error("p1", "crashed").unwrap();
        let info = mgr.get_plugin_info("p1").unwrap();
        assert_eq!(info.status, PluginStatus::Error);
        assert_eq!(info.error.as_deref(), Some("crashed"));
    }

    // -- Config tests -------------------------------------------------------

    #[test]
    fn get_set_plugin_config_merges() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        mgr.set_plugin_config("p1", &serde_json::json!({"a": 1}))
            .unwrap();
        mgr.set_plugin_config("p1", &serde_json::json!({"b": 2}))
            .unwrap();

        let config = mgr.get_plugin_config("p1").unwrap();
        assert_eq!(config["a"], 1);
        assert_eq!(config["b"], 2);
    }

    // -- Session management tests -------------------------------------------

    #[test]
    fn register_session_creates_active_session() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let session = mgr
            .register_session("p1", AgentType::Claude, None, None, None)
            .unwrap();

        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.plugin_id, "p1");
        assert!(session.agent_id.starts_with("agent-"));
        assert_eq!(mgr.list_sessions().len(), 1);
    }

    #[test]
    fn register_session_with_custom_agent_id() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let session = mgr
            .register_session(
                "p1",
                AgentType::Opencode,
                Some("my-agent".to_string()),
                Some("pane-1".to_string()),
                None,
            )
            .unwrap();

        assert_eq!(session.agent_id, "my-agent");
        assert_eq!(session.pane_id, Some("pane-1".to_string()));
    }

    #[test]
    fn register_session_rejects_unknown_plugin() {
        let mgr = PluginManager::new();
        let result = mgr.register_session("nonexistent", AgentType::Claude, None, None, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PluginError::PluginNotFound(ref id) if id == "nonexistent"
        ));
    }

    #[test]
    fn scoped_capabilities_limits_requests() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        // Shell only gets notifications + status; requesting tasks should be
        // filtered out.
        let session = mgr
            .register_session(
                "p1",
                AgentType::Shell,
                None,
                None,
                Some(vec![
                    PluginCapability::Notifications,
                    PluginCapability::Tasks,
                ]),
            )
            .unwrap();

        assert_eq!(session.capabilities, vec![PluginCapability::Notifications]);
    }

    #[test]
    fn remove_session_cleans_up() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let session = mgr
            .register_session("p1", AgentType::Claude, None, None, None)
            .unwrap();

        let sid = session.id.clone();
        mgr.remove_session(&sid).unwrap();
        assert!(mgr.get_session(&sid).is_none());
    }

    #[test]
    fn remove_session_errors_on_unknown() {
        let mgr = PluginManager::new();
        let result = mgr.remove_session("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn update_session_status() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let session = mgr
            .register_session("p1", AgentType::Claude, None, None, None)
            .unwrap();

        mgr.update_session_status(&session.id, SessionStatus::WaitingInput, None)
            .unwrap();

        let updated = mgr.get_session(&session.id).unwrap();
        assert_eq!(updated.status, SessionStatus::WaitingInput);
    }

    #[test]
    fn get_session_by_agent_id() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let session = mgr
            .register_session(
                "p1",
                AgentType::Claude,
                Some("findme".to_string()),
                None,
                None,
            )
            .unwrap();

        let found = mgr.get_session_by_agent_id("findme").unwrap();
        assert_eq!(found.id, session.id);
    }

    // -- Event subscription tests -------------------------------------------

    #[test]
    fn subscribe_and_get_subscribers() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let s1 = mgr
            .register_session("p1", AgentType::Claude, Some("a1".into()), None, None)
            .unwrap();
        let s2 = mgr
            .register_session("p1", AgentType::Claude, Some("a2".into()), None, None)
            .unwrap();

        mgr.subscribe_session(&s1.id, &[PluginEventType::Notification])
            .unwrap();
        mgr.subscribe_session(
            &s2.id,
            &[PluginEventType::Notification, PluginEventType::StatusUpdate],
        )
        .unwrap();

        let notif_subs = mgr.get_subscribers(&PluginEventType::Notification);
        assert_eq!(notif_subs.len(), 2);

        let status_subs = mgr.get_subscribers(&PluginEventType::StatusUpdate);
        assert_eq!(status_subs.len(), 1);
    }

    #[test]
    fn disconnected_sessions_excluded_from_subscribers() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let s1 = mgr
            .register_session("p1", AgentType::Claude, Some("a1".into()), None, None)
            .unwrap();
        mgr.subscribe_session(&s1.id, &[PluginEventType::Notification])
            .unwrap();

        mgr.update_session_status(&s1.id, SessionStatus::Disconnected, None)
            .unwrap();

        let subs = mgr.get_subscribers(&PluginEventType::Notification);
        assert!(subs.is_empty());
    }

    #[test]
    fn subscribe_errors_on_unknown_session() {
        let mgr = PluginManager::new();
        let result = mgr.subscribe_session("bogus", &[PluginEventType::Notification]);
        assert!(result.is_err());
    }

    // -- Event emission test ------------------------------------------------

    #[test]
    fn emit_plugin_event_returns_full_event() {
        let mgr = PluginManager::new();

        let event = mgr.emit_plugin_event(
            PluginEventType::Notification,
            PluginEventSource {
                session_id: "s1".to_string(),
                pane_id: None,
                agent_type: "claude".to_string(),
                agent_id: Some("a1".to_string()),
            },
            PluginEventPayload {
                level: Some(PayloadLevel::Info),
                message: Some("hello".to_string()),
                title: None,
                metadata: None,
                task_title: None,
                result: None,
                error: None,
                prompt: None,
                options: None,
                request_id: None,
                response: None,
                exit_code: None,
                command: None,
                session_id: None,
                agent_id: None,
                plugin_id: None,
            },
        );

        assert!(event.id.starts_with("evt-"));
        assert!(event.timestamp > 0);
        assert_eq!(event.event_type, PluginEventType::Notification);
    }

    // -- Message relay tests ------------------------------------------------

    #[test]
    fn send_and_complete_message() {
        let mgr = PluginManager::new();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let session = mgr
            .register_session("p1", AgentType::Claude, None, None, None)
            .unwrap();

        let msg = mgr
            .send_message(&session.id, "run_tool", serde_json::json!({"tool": "ls"}))
            .unwrap();

        assert!(msg.id.starts_with("msg-"));
        assert_eq!(mgr.get_pending_messages(&session.id).len(), 1);

        let completed = mgr.complete_message(&msg.id).unwrap();
        assert_eq!(completed.id, msg.id);
        assert!(mgr.get_pending_messages(&session.id).is_empty());
    }

    #[test]
    fn send_message_errors_on_unknown_session() {
        let mgr = PluginManager::new();
        let result = mgr.send_message("bogus", "method", serde_json::json!({}));
        assert!(result.is_err());
    }

    // -- Health check tests -------------------------------------------------

    #[test]
    fn health_check_marks_stalled_sessions_idle() {
        let mgr = PluginManager::new();
        // Set a very short stall timeout so sessions immediately appear stalled.
        mgr.set_stall_timeout(Duration::from_millis(0)).unwrap();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let s1 = mgr
            .register_session("p1", AgentType::Claude, Some("a1".into()), None, None)
            .unwrap();

        // Give a tiny buffer so the session's last_activity_at is in the past.
        std::thread::sleep(Duration::from_millis(2));

        let result = mgr.health_check().unwrap();
        assert_eq!(result.stalled_sessions, 1);
        assert!(result.stalled_session_ids.contains(&s1.id));

        let session = mgr.get_session(&s1.id).unwrap();
        assert_eq!(session.status, SessionStatus::Idle);
    }

    #[test]
    fn health_check_skips_disconnected_and_waiting() {
        let mgr = PluginManager::new();
        mgr.set_stall_timeout(Duration::from_millis(0)).unwrap();
        mgr.register_plugin(sample_manifest("p1")).unwrap();

        let s1 = mgr
            .register_session("p1", AgentType::Claude, Some("a1".into()), None, None)
            .unwrap();
        let s2 = mgr
            .register_session("p1", AgentType::Claude, Some("a2".into()), None, None)
            .unwrap();

        mgr.update_session_status(&s1.id, SessionStatus::Disconnected, None)
            .unwrap();
        mgr.update_session_status(&s2.id, SessionStatus::WaitingInput, None)
            .unwrap();

        std::thread::sleep(Duration::from_millis(2));

        let result = mgr.health_check().unwrap();
        assert_eq!(result.stalled_sessions, 0);
        assert_eq!(result.disconnected_sessions, 1);
    }

    // -- Plugin discovery tests ---------------------------------------------

    #[test]
    fn discover_plugins_from_directory() {
        let temp_dir = std::env::temp_dir().join("athena_plugins_test_discover");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let manifest_json = serde_json::json!({
            "id": "disc-1",
            "name": "Discovered",
            "version": "0.1.0",
            "description": "Found on disk",
            "author": "Test",
            "permissions": [],
            "capabilities": [],
            "tools": []
        });
        std::fs::write(
            temp_dir.join("disc-1.json"),
            serde_json::to_string(&manifest_json).unwrap(),
        )
        .unwrap();

        // Write an invalid file.
        std::fs::write(temp_dir.join("bad.json"), "not json").unwrap();

        // Write a non-JSON file (should be skipped).
        std::fs::write(temp_dir.join("readme.md"), "hello").unwrap();

        let mgr = PluginManager::new();
        let results = mgr.discover_plugins(&temp_dir).unwrap();

        assert_eq!(results.len(), 2); // bad.json + disc-1.json; readme.md skipped

        let ok_count = results.iter().filter(|r| r.is_ok()).count();
        let err_count = results.iter().filter(|r| r.is_err()).count();
        assert_eq!(ok_count, 1);
        assert_eq!(err_count, 1);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn discover_plugins_skips_oversized_manifests() {
        let temp_dir = std::env::temp_dir().join("athena_plugins_test_oversized");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Write a JSON file larger than MAX_MANIFEST_BYTES (1 MiB). The
        // padding is wrapped inside a JSON string field so the file is
        // syntactically valid JSON, but its size exceeds the limit and the
        // size check (which happens before parsing) should skip it.
        let padding_len = (MAX_MANIFEST_BYTES as usize) + 1024;
        let padding: String = std::iter::repeat('a').take(padding_len).collect();
        let oversized = serde_json::json!({
            "id": "huge",
            "name": "Huge",
            "version": "0.1.0",
            "description": padding,
            "author": "Test",
            "permissions": [],
            "capabilities": [],
            "tools": []
        });
        std::fs::write(
            temp_dir.join("huge.json"),
            serde_json::to_string(&oversized).unwrap(),
        )
        .unwrap();

        // Also write a small valid manifest to confirm normal parsing still works.
        let small = serde_json::json!({
            "id": "small",
            "name": "Small",
            "version": "0.1.0",
            "description": "tiny",
            "author": "Test",
            "permissions": [],
            "capabilities": [],
            "tools": []
        });
        std::fs::write(
            temp_dir.join("small.json"),
            serde_json::to_string(&small).unwrap(),
        )
        .unwrap();

        let mgr = PluginManager::new();
        let results = mgr.discover_plugins(&temp_dir).unwrap();

        // Only the small manifest should be returned; the oversized one is
        // skipped silently (no entry in results) — the discovery loop
        // `continue`s before pushing a result.
        assert_eq!(results.len(), 1);
        let manifest = results[0].as_ref().expect("small.json parses");
        assert_eq!(manifest.id, "small");

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn discover_and_register_from_directory() {
        let temp_dir = std::env::temp_dir().join("athena_plugins_test_auto_reg");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let manifest_json = serde_json::json!({
            "id": "auto-1",
            "name": "Auto Registered",
            "version": "0.1.0",
            "description": "Found on disk",
            "author": "Test",
            "permissions": [],
            "capabilities": [],
            "tools": []
        });
        std::fs::write(
            temp_dir.join("auto-1.json"),
            serde_json::to_string(&manifest_json).unwrap(),
        )
        .unwrap();

        let mgr = PluginManager::new();
        let (registered, errors) = mgr.discover_and_register(&temp_dir).unwrap();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0], "auto-1");
        assert!(errors.is_empty());

        assert_eq!(mgr.list_plugins().len(), 1);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    // -- Callback counting test ---------------------------------------------

    #[test]
    fn callbacks_are_invoked() {
        let cb = Arc::new(CountingCallbacks::new());
        let mgr = PluginManager::with_callbacks(cb.clone());

        mgr.register_plugin(sample_manifest("p1")).unwrap();
        assert_eq!(cb.plugin_registered.load(Ordering::Relaxed), 1);
        assert_eq!(cb.registry_updated.load(Ordering::Relaxed), 1);

        let session = mgr
            .register_session("p1", AgentType::Claude, None, None, None)
            .unwrap();
        assert_eq!(cb.session_registered.load(Ordering::Relaxed), 1);

        mgr.remove_session(&session.id).unwrap();
        assert_eq!(cb.session_removed.load(Ordering::Relaxed), 1);
    }

    // -- Default capabilities test ------------------------------------------

    #[test]
    fn default_capabilities_by_agent_type() {
        let claude_caps = default_capabilities(&AgentType::Claude);
        assert!(claude_caps.contains(&PluginCapability::Tasks));

        let shell_caps = default_capabilities(&AgentType::Shell);
        assert!(!shell_caps.contains(&PluginCapability::Tasks));
        assert!(shell_caps.contains(&PluginCapability::Notifications));
    }

    // -- Manifest validation tests -----------------------------------------

    #[test]
    fn validate_manifest_accepts_builtin() {
        let mut manifest = sample_manifest("v1");
        manifest.install = Some(PluginInstallMethod::Builtin);
        assert!(validate_plugin_manifest(&manifest).is_ok());
    }

    #[test]
    fn validate_hook_script_rejects_absolute_path() {
        let mut manifest = sample_manifest("v2");
        manifest.install = Some(PluginInstallMethod::Hook {
            script: "/usr/bin/malicious".to_string(),
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn validate_hook_script_rejects_path_traversal() {
        let mut manifest = sample_manifest("v3");
        manifest.install = Some(PluginInstallMethod::Hook {
            script: "../escape.sh".to_string(),
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("traverse"));
    }

    #[test]
    fn validate_hook_script_rejects_shell_metacharacters() {
        let cases = [
            (";rm -rf", ';'),
            ("ls | grep", '|'),
            ("bg &", '&'),
            ("echo $HOME", '$'),
            ("echo `whoami`", '`'),
        ];
        for (script, ch) in cases {
            let mut manifest = sample_manifest("v4");
            manifest.install = Some(PluginInstallMethod::Hook {
                script: script.to_string(),
            });
            let err = validate_plugin_manifest(&manifest).unwrap_err();
            assert!(
                err.to_string().contains("metacharacter"),
                "expected metacharacter error for '{}', got: {}",
                ch,
                err
            );
        }
    }

    #[test]
    fn validate_hook_script_accepts_simple_relative_path() {
        let mut manifest = sample_manifest("v5");
        manifest.install = Some(PluginInstallMethod::Hook {
            script: "scripts/setup.sh".to_string(),
        });
        assert!(validate_plugin_manifest(&manifest).is_ok());
    }

    #[test]
    fn validate_mcp_command_rejects_absolute_path() {
        let mut manifest = sample_manifest("v6");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "/usr/local/bin/node".to_string(),
            args: None,
            env: None,
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("absolute path"));
    }

    #[test]
    fn validate_mcp_command_rejects_dot_slash() {
        let mut manifest = sample_manifest("v7");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "./malicious".to_string(),
            args: None,
            env: None,
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("relative path"));
    }

    #[test]
    fn validate_mcp_command_rejects_path_with_separator() {
        let mut manifest = sample_manifest("v8");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "bin/node".to_string(),
            args: None,
            env: None,
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("'/'"));
    }

    #[test]
    fn validate_mcp_command_rejects_unknown_executable() {
        let mut manifest = sample_manifest("v9");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "curl".to_string(),
            args: None,
            env: None,
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[test]
    fn validate_mcp_command_accepts_whitelisted_executables() {
        for cmd in &[
            "node", "python", "python3", "ruby", "cargo", "sh", "bash", "zsh",
        ] {
            let mut manifest = sample_manifest("v10");
            manifest.install = Some(PluginInstallMethod::McpServer {
                command: cmd.to_string(),
                args: None,
                env: None,
            });
            assert!(
                validate_plugin_manifest(&manifest).is_ok(),
                "expected '{}' to be allowed",
                cmd
            );
        }
    }

    #[test]
    fn validate_mcp_env_rejects_path_override() {
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), "/malicious".to_string());
        let mut manifest = sample_manifest("v11");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "node".to_string(),
            args: None,
            env: Some(env),
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("PATH"));
    }

    #[test]
    fn validate_mcp_env_rejects_home_override() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/malicious".to_string());
        let mut manifest = sample_manifest("v12");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "node".to_string(),
            args: None,
            env: Some(env),
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("HOME"));
    }

    #[test]
    fn validate_mcp_env_allows_safe_vars() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());
        let mut manifest = sample_manifest("v13");
        manifest.install = Some(PluginInstallMethod::McpServer {
            command: "node".to_string(),
            args: None,
            env: Some(env),
        });
        assert!(validate_plugin_manifest(&manifest).is_ok());
    }

    #[test]
    fn validate_mcp_config_also_validates() {
        let mut manifest = sample_manifest("v14");
        manifest.mcp_config = Some(McpConfig {
            command: "/usr/bin/wget".to_string(),
            args: vec![],
            env: None,
        });
        let err = validate_plugin_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("absolute path"));
    }

    #[test]
    fn register_plugin_rejects_unsafe_manifest() {
        let mut manifest = sample_manifest("v15");
        manifest.install = Some(PluginInstallMethod::Hook {
            script: "/usr/bin/evil".to_string(),
        });
        let mgr = PluginManager::new();
        let result = mgr.register_plugin(manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute"));
    }
}
