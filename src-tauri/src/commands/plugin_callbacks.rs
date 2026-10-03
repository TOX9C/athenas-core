//! Bridge from `athena_plugins::PluginCallbacks` to Tauri renderer events.
//!
//! Emits the six `plugin:*` channels the frontend bus
//! (`plugin_event_bus.rs`) and the relay forwarding table
//! (`relay/ws.rs`) already consume — no new channel names.
//!
//! Payload contract (JSON strings, verbatim from the bus parser):
//! - `plugin:registryUpdated` → `{"registry": [PluginInfo...]}`
//! - `plugin:registered`      → `{"pluginId","name","version"}`
//! - `plugin:enabled|disabled`→ `{"pluginId"}`
//! - `plugin:error`           → `{"pluginId","error"}`
//! - `plugin:event`           → raw serialized `PluginEvent` (bus stores verbatim)

use std::collections::HashMap;
use std::sync::Arc;

use athena_plugins::{PluginCallbacks, PluginEvent, PluginInfo, PluginSession};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

/// Real [`PluginCallbacks`] that emits Tauri events for plugin lifecycle
/// hooks. Holds the shared late-bound app handle (set once the app is
/// running); hooks arriving before that are dropped silently.
pub struct TauriPluginCallbacks {
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl TauriPluginCallbacks {
    pub fn new(app_handle: Arc<Mutex<Option<AppHandle>>>) -> Self {
        Self { app_handle }
    }

    fn emit(&self, channel: &'static str, payload: serde_json::Value) {
        let Some(handle) = self.app_handle.lock().clone() else {
            return;
        };
        match serde_json::to_string(&payload) {
            Ok(payload_str) => {
                if let Err(e) = handle.emit(channel, payload_str) {
                    log::warn!("failed to emit {channel} event: {e}");
                }
            }
            Err(e) => log::warn!("failed to serialize {channel} payload: {e}"),
        }
    }
}

impl PluginCallbacks for TauriPluginCallbacks {
    // Session hooks have no frontend/relay consumer today — intentionally
    // no-op rather than emit channels nobody consumes.
    // ponytail: emit plugin:session* channels when a consumer ships.
    fn on_session_registered(&self, _session: &PluginSession) {}

    fn on_session_removed(&self, _session_id: &str, _agent_id: &str) {}

    fn on_session_status_update(
        &self,
        _session_id: &str,
        _agent_id: &str,
        _status: athena_plugins::SessionStatus,
        _data: Option<&serde_json::Value>,
    ) {
    }

    fn on_plugin_event(&self, event: &PluginEvent) {
        // The bus stores this payload verbatim ("consumers parse this exact
        // shape"), so hand it the serialized event as-is.
        let Some(handle) = self.app_handle.lock().clone() else {
            return;
        };
        match serde_json::to_string(event) {
            Ok(payload_str) => {
                if let Err(e) = handle.emit("plugin:event", payload_str) {
                    log::warn!("failed to emit plugin:event: {e}");
                }
            }
            Err(e) => log::warn!("failed to serialize plugin:event payload: {e}"),
        }
    }

    fn on_registry_updated(&self, registry: &HashMap<String, PluginInfo>) {
        // Frontend parses entries as {id,name,version,status,error?} —
        // PluginInfo's serde shape matches exactly.
        let registry: Vec<&PluginInfo> = registry.values().collect();
        self.emit("plugin:registryUpdated", serde_json::json!({ "registry": registry }));
    }

    fn on_plugin_registered(&self, plugin_id: &str, name: &str) {
        self.emit(
            "plugin:registered",
            serde_json::json!({
                "pluginId": plugin_id,
                "name": name,
                "version": "",
            }),
        );
    }

    fn on_plugin_enabled(&self, plugin_id: &str, _name: &str) {
        self.emit("plugin:enabled", serde_json::json!({ "pluginId": plugin_id }));
    }

    fn on_plugin_disabled(&self, plugin_id: &str) {
        self.emit("plugin:disabled", serde_json::json!({ "pluginId": plugin_id }));
    }

    fn on_plugin_error(&self, plugin_id: &str, error: &str) {
        self.emit(
            "plugin:error",
            serde_json::json!({ "pluginId": plugin_id, "error": error }),
        );
    }

    fn on_plugin_configured(&self, _plugin_id: &str, _config: &serde_json::Value) {}
}
