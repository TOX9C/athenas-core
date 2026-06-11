//! Plugin management crate for Athena's Core.
//!
//! Mirrors the Electron `pluginHost.ts` and `plugin-manager.ts` services,
//! providing plugin discovery, registration, session management, event relay,
//! and health checking as a pure data/coordination layer. Actual TCP/network
//! communication with external plugins is handled by the src-tauri commands
//! that call into this manager.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
// Enum types
// ---------------------------------------------------------------------------

/// Capabilities a plugin can advertise. Mirrors the TS `PluginCapability`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    Notifications,
    Status,
    Tasks,
    AgentControl,
    UserInput,
    FileAccess,
    Swarm,
}

/// Runtime status of an installed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Installed,
    Enabled,
    Disabled,
    Error,
}

/// Status of a plugin session. Mirrors the TS session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Idle,
    WaitingInput,
    Disconnected,
}

/// Plugin event types. Mirrors the TS `PluginEventType`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginEventType {
    Notification,
    StatusUpdate,
    TaskComplete,
    TaskError,
    NeedsInput,
    AgentSpawned,
    AgentExited,
    AgentStalled,
    ProgressUpdate,
    ArtifactProduced,
    UserResponse,
    ControlCommand,
    AgentConnected,
    AgentDisconnected,
    PluginRegistered,
    PluginError,
    OutputForwarded,
}

/// Type of AI agent a pane can host.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    Codex,
    Opencode,
    Gemini,
    Custom,
    Shell,
}

/// Level for a plugin event payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadLevel {
    Info,
    Warning,
    Error,
    Success,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// How a plugin is installed and invoked. Mirrors the TS `PluginInstallMethod`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginInstallMethod {
    Builtin,
    McpServer {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    Hook {
        script: String,
    },
}

/// Schema and defaults for plugin configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginConfigSchema {
    pub schema: serde_json::Value,
    pub defaults: serde_json::Value,
}

/// Definition of a tool exposed by a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub capability: PluginCapability,
    pub phase: u8,
}

/// MCP server configuration embedded in a manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// Full manifest of a plugin, typically read from a JSON file on disk.
/// Mirrors the TS `PluginManifest` from `plugin-manager.ts` and `plugin.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<PluginCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_athena_version: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    #[serde(default)]
    pub tools: Vec<PluginToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribes_to: Option<Vec<PluginEventType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PluginConfigSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<PluginInstallMethod>,
}

/// An installed plugin record combining its manifest with runtime state.
/// Mirrors the TS `PluginEntry` from `plugin-manager.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub installed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_enabled_at: Option<i64>,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Public-facing plugin info, safe to expose to the renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub status: PluginStatus,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A plugin session representing a live connection between an agent and a plugin.
/// Mirrors the TS `PluginSession` from `pluginHost.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSession {
    pub id: String,
    pub plugin_id: String,
    pub agent_type: AgentType,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub capabilities: Vec<PluginCapability>,
    pub connected_at: i64,
    pub last_activity_at: i64,
    pub status: SessionStatus,
}

/// Source metadata for a plugin event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEventSource {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub agent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// The rich payload of a plugin event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEventPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<PayloadLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

/// A full plugin event with generated ID and timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: PluginEventType,
    pub source: PluginEventSource,
    pub payload: PluginEventPayload,
    pub timestamp: i64,
}

/// A pending message awaiting a response from a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub id: String,
    pub session_id: String,
    pub method: String,
    pub params: serde_json::Value,
    pub sent_at: i64,
}

/// Result of a health check pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub checked_at: i64,
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub idle_sessions: usize,
    pub stalled_sessions: usize,
    pub disconnected_sessions: usize,
    pub stalled_session_ids: Vec<String>,
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

struct PluginManagerInner {
    plugins: HashMap<String, PluginEntry>,
    sessions: HashMap<String, PluginSession>,
    event_subscriptions: HashMap<PluginEventType, HashSet<String>>,
    pending_messages: HashMap<String, PendingMessage>,
    stall_timeout: Duration,
    last_health_check: Option<Instant>,
}

// ---------------------------------------------------------------------------
// Manifest validation
// ---------------------------------------------------------------------------

/// Allowed executable names for MCP server commands.
const ALLOWED_MCP_COMMANDS: &[&str] = &[
    "node", "python", "python3", "ruby", "cargo", "sh", "bash", "zsh", "npx", "deno", "uv", "uvx",
    "pipx",
];

/// Shell metacharacters that indicate injection risk in hook scripts.
const SHELL_METACHARACTERS: &[char] = &[';', '|', '&', '$', '`', '\n'];

/// Validate a plugin manifest before registration.
///
/// Checks `install` and `mcp_config` fields for unsafe values that could
/// lead to arbitrary code execution:
///
/// - **Hook scripts**: must be simple relative paths (no metacharacters,
///   no absolute paths, no path traversal).
/// - **MCP commands**: must be a whitelisted executable name (no absolute
///   paths, no `./` prefixes).
/// - **MCP env**: must not override `PATH` or `HOME`.
pub fn validate_plugin_manifest(manifest: &PluginManifest) -> Result<(), PluginError> {
    // Validate the install method if present.
    if let Some(ref install) = manifest.install {
        validate_plugin_install_method(install)?;
    }

    // Validate the embedded mcp_config if present.
    if let Some(ref mcp_config) = manifest.mcp_config {
        validate_mcp_command(&mcp_config.command)?;
        if let Some(ref env) = mcp_config.env {
            validate_mcp_env(env)?;
        }
    }

    Ok(())
}

/// Validate a [`PluginInstallMethod`].
pub fn validate_plugin_install_method(method: &PluginInstallMethod) -> Result<(), PluginError> {
    match method {
        PluginInstallMethod::Builtin => Ok(()),
        PluginInstallMethod::McpServer {
            command,
            args: _,
            env,
        } => {
            validate_mcp_command(command)?;
            if let Some(ref env_map) = env {
                validate_mcp_env(env_map)?;
            }
            Ok(())
        }
        PluginInstallMethod::Hook { script } => validate_hook_script(script),
    }
}

fn validate_hook_script(script: &str) -> Result<(), PluginError> {
    let path = Path::new(script);

    // Reject absolute paths (has root component or drive letter on Windows).
    if path.is_absolute() || path.has_root() {
        return Err(PluginError::ValidationFailed(format!(
            "hook script must be a relative path, got absolute: {script}"
        )));
    }

    // Reject path traversal (.. in any form, including Windows ..\).
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(PluginError::ValidationFailed(format!(
            "hook script must not traverse parent directories: {script}"
        )));
    }

    // Reject shell metacharacters.
    if let Some(pos) = script
        .chars()
        .position(|c| SHELL_METACHARACTERS.contains(&c))
    {
        let ch = script.chars().nth(pos).unwrap_or('?');
        return Err(PluginError::ValidationFailed(format!(
            "hook script contains shell metacharacter '{}': {script}",
            ch
        )));
    }

    Ok(())
}

fn validate_mcp_command(command: &str) -> Result<(), PluginError> {
    // Reject absolute paths.
    if command.starts_with('/') {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command must be a bare executable name, got absolute path: {command}"
        )));
    }

    // Reject relative path prefixes (e.g. "./malicious").
    if command.starts_with("./") || command.starts_with("../") {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command must be a bare executable name, got relative path: {command}"
        )));
    }

    // Reject if it contains a directory separator (e.g. "bin/node").
    if command.contains('/') {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command must be a bare executable name, got path with '/': {command}"
        )));
    }

    // Must be on the whitelist.
    if !ALLOWED_MCP_COMMANDS.contains(&command) {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command '{}' is not allowed. Permitted commands: {}",
            command,
            ALLOWED_MCP_COMMANDS.join(", ")
        )));
    }

    Ok(())
}

fn validate_mcp_env(env: &HashMap<String, String>) -> Result<(), PluginError> {
    let forbidden = ["PATH", "HOME"];
    for key in env.keys() {
        if forbidden.contains(&key.as_str()) {
            return Err(PluginError::ValidationFailed(format!(
                "MCP env must not override '{key}'"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// PluginManager
// ---------------------------------------------------------------------------

/// Thread-safe plugin manager. Wraps all mutable state in a single
/// `Arc<Mutex<PluginManagerInner>>` to avoid nested locking and deadlocks.
pub struct PluginManager {
    inner: Arc<Mutex<PluginManagerInner>>,
    callbacks: Arc<dyn PluginCallbacks>,
    event_emitter: Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
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

    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
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

    // -- Plugin discovery ---------------------------------------------------

    /// Scan a directory for plugin manifest JSON files and return the parsed
    /// manifests. Files must have a `.json` extension and contain a valid
    /// [`PluginManifest`]. Invalid files are skipped (their errors are
    /// captured in the returned vector).
    pub fn discover_plugins(
        &self,
        dir: &Path,
    ) -> Result<Vec<Result<PluginManifest, PluginError>>, PluginError> {
        let entries = fs::read_dir(dir)?;
        let mut results = Vec::new();

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    results.push(Err(PluginError::ManifestIo(e)));
                    continue;
                }
            };

            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            // Reject oversized manifests before reading to bound memory usage.
            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.len() > MAX_MANIFEST_BYTES {
                log::warn!(
                    "Skipping oversized plugin manifest ({} bytes): {}",
                    metadata.len(),
                    path.display()
                );
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    results.push(Err(PluginError::ManifestIo(e)));
                    continue;
                }
            };

            match serde_json::from_str::<PluginManifest>(&content) {
                Ok(manifest) => {
                    if let Err(e) = validate_plugin_manifest(&manifest) {
                        results.push(Err(PluginError::ValidationFailed(format!(
                            "manifest {} failed validation: {}",
                            path.to_string_lossy(),
                            e
                        ))));
                    } else {
                        results.push(Ok(manifest));
                    }
                }
                Err(e) => results.push(Err(PluginError::ManifestParse {
                    path: path.to_string_lossy().into_owned(),
                    source: e,
                })),
            }
        }

        Ok(results)
    }

    /// Discover and automatically register all valid plugins in a directory.
    /// Returns the IDs of successfully registered plugins and any errors.
    pub fn discover_and_register(
        &self,
        dir: &Path,
    ) -> Result<(Vec<String>, Vec<PluginError>), PluginError> {
        let discovered = self.discover_plugins(dir)?;
        let mut registered = Vec::new();
        let mut errors = Vec::new();

        for result in discovered {
            match result {
                Ok(manifest) => match self.register_plugin(manifest) {
                    Ok(id) => registered.push(id),
                    Err(e) => errors.push(e),
                },
                Err(e) => errors.push(e),
            }
        }

        Ok((registered, errors))
    }

    // -- Plugin registration ------------------------------------------------

    /// Register a plugin from its manifest. Returns the plugin ID on success.
    /// Fails if a plugin with the same ID is already registered and not
    /// disabled (mirrors the TS behavior of rejecting double-registration
    /// of active plugins), or if the manifest fails security validation.
    pub fn register_plugin(&self, manifest: PluginManifest) -> Result<String, PluginError> {
        // Validate install method and MCP config before accepting.
        validate_plugin_manifest(&manifest)?;

        let id = manifest.id.clone();

        let mut inner = self.inner.lock()?;

        if let Some(existing) = inner.plugins.get(&id) {
            if existing.status != PluginStatus::Disabled {
                return Err(PluginError::AlreadyRegistered(id));
            }
        }

        let now = now_millis();
        let name = manifest.name.clone();
        let entry = PluginEntry {
            manifest,
            status: PluginStatus::Installed,
            installed_at: now,
            last_enabled_at: None,
            config: serde_json::Value::Object(serde_json::Map::new()),
            error: None,
        };

        inner.plugins.insert(id.clone(), entry);

        let registry_info = build_registry_info(&inner);

        drop(inner);

        self.callbacks.on_plugin_registered(&id, &name);
        self.emit_registry_update(registry_info);

        self.emit_event(
            "plugin:registered",
            &serde_json::json!({
                "pluginId": id,
                "name": name,
            }),
        );

        Ok(id)
    }

    /// Unregister a plugin by ID. Returns `Ok(())` if the plugin existed
    /// and was removed, or an error if the plugin was not found.
    pub fn unregister_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let was_enabled = inner
            .plugins
            .get(plugin_id)
            .map(|e| e.status == PluginStatus::Enabled)
            .ok_or_else(|| PluginError::PluginNotFound(plugin_id.to_string()))?;

        inner.plugins.remove(plugin_id);

        let registry_info = build_registry_info(&inner);

        drop(inner);

        if was_enabled {
            self.callbacks.on_plugin_disabled(plugin_id);
        }
        self.emit_registry_update(registry_info);

        Ok(())
    }

    /// Enable a previously installed/disabled plugin.
    pub fn enable_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let entry = inner
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::PluginNotFound(plugin_id.to_string()))?;

        if entry.status == PluginStatus::Enabled {
            return Ok(());
        }

        entry.status = PluginStatus::Enabled;
        entry.last_enabled_at = Some(now_millis());
        entry.error = None;
        let name = entry.manifest.name.clone();

        let registry_info = build_registry_info(&inner);

        drop(inner);

        self.callbacks.on_plugin_enabled(plugin_id, &name);
        self.emit_registry_update(registry_info);

        self.emit_event(
            "plugin:enabled",
            &serde_json::json!({
                "pluginId": plugin_id,
                "name": name,
            }),
        );

        Ok(())
    }

    /// Disable an enabled plugin.
    pub fn disable_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let entry = inner
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::PluginNotFound(plugin_id.to_string()))?;

        if entry.status == PluginStatus::Disabled {
            return Ok(());
        }

        let was_enabled = entry.status == PluginStatus::Enabled;
        entry.status = PluginStatus::Disabled;

        let registry_info = build_registry_info(&inner);

        drop(inner);

        if was_enabled {
            self.callbacks.on_plugin_disabled(plugin_id);
            self.emit_event(
                "plugin:disabled",
                &serde_json::json!({
                    "pluginId": plugin_id,
                }),
            );
        }
        self.emit_registry_update(registry_info);

        Ok(())
    }

    /// Set a plugin to error status with an error message.
    pub fn set_plugin_error(&self, plugin_id: &str, error: &str) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let entry = inner
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::PluginNotFound(plugin_id.to_string()))?;

        entry.status = PluginStatus::Error;
        entry.error = Some(error.to_string());

        let registry_info = build_registry_info(&inner);

        drop(inner);

        self.callbacks.on_plugin_error(plugin_id, error);
        self.emit_registry_update(registry_info);

        self.emit_event(
            "plugin:error",
            &serde_json::json!({
                "pluginId": plugin_id,
                "error": error,
            }),
        );

        Ok(())
    }

    // -- Plugin listing & querying ------------------------------------------

    /// List all registered plugins as public-facing info.
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner
            .plugins
            .values()
            .map(|e| PluginInfo {
                id: e.manifest.id.clone(),
                name: e.manifest.name.clone(),
                version: e.manifest.version.clone(),
                description: e.manifest.description.clone(),
                author: e.manifest.author.clone(),
                status: e.status,
                config: e.config.clone(),
                error: e.error.clone(),
            })
            .collect()
    }

    /// Get a single plugin entry by ID.
    pub fn get_plugin(&self, plugin_id: &str) -> Option<PluginEntry> {
        let inner = self.inner.lock().ok()?;
        inner.plugins.get(plugin_id).cloned()
    }

    /// Get a public-facing plugin info by ID.
    pub fn get_plugin_info(&self, plugin_id: &str) -> Option<PluginInfo> {
        let inner = self.inner.lock().ok()?;
        inner.plugins.get(plugin_id).map(|e| PluginInfo {
            id: e.manifest.id.clone(),
            name: e.manifest.name.clone(),
            version: e.manifest.version.clone(),
            description: e.manifest.description.clone(),
            author: e.manifest.author.clone(),
            status: e.status,
            config: e.config.clone(),
            error: e.error.clone(),
        })
    }

    /// Get all enabled plugins.
    pub fn get_enabled_plugins(&self) -> Vec<PluginEntry> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner
            .plugins
            .values()
            .filter(|e| e.status == PluginStatus::Enabled)
            .cloned()
            .collect()
    }

    /// Get plugin configuration.
    pub fn get_plugin_config(&self, plugin_id: &str) -> Option<serde_json::Value> {
        let inner = self.inner.lock().ok()?;
        inner.plugins.get(plugin_id).map(|e| e.config.clone())
    }

    /// Update plugin configuration (merges with existing config).
    pub fn set_plugin_config(
        &self,
        plugin_id: &str,
        config: &serde_json::Value,
    ) -> Result<(), PluginError> {
        let mut inner = self.inner.lock()?;

        let entry = inner
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::PluginNotFound(plugin_id.to_string()))?;

        // Merge: existing config is the base, new config overwrites.
        match (&mut entry.config, config) {
            (serde_json::Value::Object(ref mut existing), serde_json::Value::Object(ref new)) => {
                for (key, value) in new {
                    existing.insert(key.clone(), value.clone());
                }
            }
            (_, new_config) => {
                entry.config = new_config.clone();
            }
        }

        let merged_config = entry.config.clone();

        let registry_info = build_registry_info(&inner);

        drop(inner);

        self.callbacks
            .on_plugin_configured(plugin_id, &merged_config);
        self.emit_registry_update(registry_info);

        Ok(())
    }

    // -- Session management -------------------------------------------------

    /// Create a new session for an agent communicating with a plugin.
    /// Returns the newly created session. Mirrors `registerSession` in the
    /// TS `pluginHost.ts`.
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

    /// Get a session by its ID.
    pub fn get_session(&self, session_id: &str) -> Option<PluginSession> {
        let inner = self.inner.lock().ok()?;
        inner.sessions.get(session_id).cloned()
    }

    /// Find a session by agent ID.
    pub fn get_session_by_agent_id(&self, agent_id: &str) -> Option<PluginSession> {
        let inner = self.inner.lock().ok()?;
        inner
            .sessions
            .values()
            .find(|s| s.agent_id == agent_id)
            .cloned()
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<PluginSession> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner.sessions.values().cloned().collect()
    }

    /// Remove (close) a session by ID. Marks it as disconnected first,
    /// then removes it from the session map. Mirrors `removeSession` in
    /// the TS `pluginHost.ts`.
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

    /// Update a session's status. Mirrors `updateSessionStatus` in the TS.
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

    // -- Event subscriptions ------------------------------------------------

    /// Subscribe a session to one or more event types. Mirrors
    /// `subscribeSession` in the TS `pluginHost.ts`.
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

    /// Get all sessions subscribed to a particular event type.
    /// Mirrors `getSubscribers` in the TS `pluginHost.ts`.
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

    // -- Event emission / message relay -------------------------------------

    /// Emit a plugin event. Generates the event ID and timestamp, calls the
    /// callback, and returns the full event. Mirrors `emitPluginEvent` in
    /// the TS `pluginHost.ts`.
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

    /// Send a message to a plugin session and track it as a pending message
    /// awaiting a response. Returns the pending message record. The actual
    /// network delivery is handled by the Tauri command layer.
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

    /// Complete a pending message, removing it from the tracking map.
    /// Returns the pending message if it existed.
    pub fn complete_message(&self, message_id: &str) -> Option<PendingMessage> {
        let mut inner = self.inner.lock().ok()?;
        inner.pending_messages.remove(message_id)
    }

    /// Get all pending messages for a given session.
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

    // -- Health checking ----------------------------------------------------

    /// Perform a health check on all sessions. Sessions that have been
    /// inactive (not `Disconnected` or `WaitingInput`) for longer than the
    /// stall timeout are marked `Idle`. Mirrors the interval-based stall
    /// detection in the TS `pluginHost.ts`.
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

    /// Get the time of the last health check, if one has been performed.
    pub fn last_health_check(&self) -> Option<Instant> {
        let inner = self.inner.lock().ok()?;
        inner.last_health_check
    }

    // -- Private helpers ----------------------------------------------------

    fn emit_registry_update(&self, registry_info: HashMap<String, PluginInfo>) {
        self.callbacks.on_registry_updated(&registry_info);

        let registry_array: Vec<serde_json::Value> = registry_info
            .values()
            .map(|info| {
                serde_json::json!({
                    "id": info.id,
                    "name": info.name,
                    "version": info.version,
                    "description": info.description,
                    "author": info.author,
                    "status": info.status,
                    "config": info.config,
                    "error": info.error,
                })
            })
            .collect();

        self.emit_event(
            "plugin:registryUpdated",
            &serde_json::json!({
                "registry": registry_array,
            }),
        );
    }
}

/// Build a `HashMap<String, PluginInfo>` from the current state while holding
/// the lock, so callers can release the lock before invoking callbacks.
fn build_registry_info(
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
        AgentType::Claude | AgentType::Codex | AgentType::Opencode | AgentType::Gemini => vec![
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
fn scoped_capabilities(
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
fn now_millis() -> i64 {
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
