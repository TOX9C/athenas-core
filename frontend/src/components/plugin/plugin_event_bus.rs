use crate::components::shared::toast::{use_toast_store, Toast, ToastType};
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Plugin registry entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub error: Option<String>,
}

/// Plugin event bus state.
#[derive(Clone, PartialEq, Default)]
pub struct PluginBusState {
    pub plugins: Vec<PluginEntry>,
    pub events: Vec<String>,
}

impl PluginBusState {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Apply a parsed backend plugin event to the store.
    pub fn apply_plugin_event(&mut self, event: &PluginBusEvent) {
        match event {
            PluginBusEvent::RegistryUpdated(entries) => {
                for entry in entries {
                    self.upsert_plugin(
                        entry.id.clone(),
                        entry.name.clone(),
                        entry.version.clone(),
                        entry.enabled,
                        entry.error.clone(),
                    );
                }
            }
            PluginBusEvent::Registered { id, name, version } => {
                self.upsert_plugin(id.clone(), name.clone(), version.clone(), true, None);
            }
            PluginBusEvent::Enabled { id } => {
                self.set_plugin_enabled(id, true);
            }
            PluginBusEvent::Disabled { id } => {
                self.set_plugin_enabled(id, false);
            }
            PluginBusEvent::Error { id, error } => {
                self.set_plugin_error(id, error.clone());
            }
            PluginBusEvent::AddEvent(payload) => {
                self.add_event(payload.clone());
            }
        }
    }

    pub fn upsert_plugin(
        &mut self,
        id: String,
        name: String,
        version: String,
        enabled: bool,
        error: Option<String>,
    ) {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.name = name;
            p.version = version;
            p.enabled = enabled;
            p.error = error;
        } else {
            self.plugins.push(PluginEntry {
                id,
                name,
                version,
                enabled,
                error,
            });
        }
    }

    pub fn set_plugin_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.enabled = enabled;
            p.error = None;
        }
    }

    pub fn set_plugin_error(&mut self, id: &str, error: String) {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.error = Some(error);
        }
    }

    pub fn add_event(&mut self, event: String) {
        self.events.push(event);
        if self.events.len() > 100 {
            self.events.drain(0..self.events.len() - 100);
        }
    }
}

/// Obtain the plugin bus signal from the Dioxus context.
pub fn use_plugin_bus_store() -> Signal<PluginBusState> {
    use_context::<Signal<PluginBusState>>()
}

/// Initialize the plugin bus store as a context provider.
pub fn provide_plugin_bus_store() {
    use_context_provider(|| Signal::new(PluginBusState::new()));
}

/// Events that the plugin bus can receive from the Tauri backend.
///
/// Each variant carries the already-parsed payload. Signal writes happen
/// inside the coroutine, not inside the Tauri listen callback, to avoid
/// panics from writing to a signal while a read lock is held elsewhere.
#[derive(Debug)]
enum PluginBusEvent {
    RegistryUpdated(Vec<PluginEntry>),
    Registered {
        id: String,
        name: String,
        version: String,
    },
    Enabled {
        id: String,
    },
    Disabled {
        id: String,
    },
    Error {
        id: String,
        error: String,
    },
    AddEvent(String),
}

/// Parse a Tauri event payload from the backend plugin manager into a bus
/// event.
///
/// Wire contract: the emitter is `crates/athena-plugins`, where
/// `plugin:registered` / `plugin:enabled` / `plugin:disabled` / `plugin:error`
/// identify the plugin with the key `pluginId` (NOT `id`), and
/// `plugin:registryUpdated` wraps its entries in `{ "registry": [...] }`.
/// Registry entries carry `status` ("installed" | "enabled" | "disabled" |
/// "error") rather than an `enabled` boolean. The relay in
/// `src-tauri/src/relay/ws.rs` forwards these same payloads verbatim to paired
/// phones, so both consumers parse this exact shape — changing it requires
/// changing all three sides.
///
/// Payloads that do not match the emitted shape return `None` (event
/// dropped), which is what made the previous `id`-keyed parser silently skip
/// every live plugin update.
fn parse_plugin_bus_event(channel: &str, payload: &str) -> Option<PluginBusEvent> {
    // `plugin:event` is consumed verbatim (its inner payload is a plugin
    // event object with its own `id`, see runtime.rs `emit_plugin_event`).
    if channel == "plugin:event" {
        return Some(PluginBusEvent::AddEvent(payload.to_string()));
    }

    let val: serde_json::Value = serde_json::from_str(payload).ok()?;

    let plugin_id = |v: &serde_json::Value| -> Option<String> {
        v.get("pluginId")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|id| !id.is_empty())
    };

    match channel {
        "plugin:registryUpdated" => {
            let plugins_arr = val.get("registry")?.as_array()?;
            let mut entries = Vec::with_capacity(plugins_arr.len());
            for p in plugins_arr {
                let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let version = p
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let enabled = p.get("status").and_then(|v| v.as_str()) == Some("enabled");
                let error = p
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                entries.push(PluginEntry {
                    id,
                    name,
                    version,
                    enabled,
                    error,
                });
            }
            Some(PluginBusEvent::RegistryUpdated(entries))
        }
        "plugin:registered" => {
            let id = plugin_id(&val)?;
            let name = val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Plugin")
                .to_string();
            let version = val
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(PluginBusEvent::Registered { id, name, version })
        }
        "plugin:enabled" => plugin_id(&val).map(|id| PluginBusEvent::Enabled { id }),
        "plugin:disabled" => plugin_id(&val).map(|id| PluginBusEvent::Disabled { id }),
        "plugin:error" => {
            let id = plugin_id(&val)?;
            let error = val
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            Some(PluginBusEvent::Error { id, error })
        }
        _ => None,
    }
}

/// Plugin event bus component — renders nothing, handles IPC events.
#[component]
pub fn PluginEventBus() -> Element {
    let plugin_store = use_plugin_bus_store();
    let toast_store = use_toast_store();
    let mut mounted = use_signal(|| false);

    let unlistens: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    // Dispatcher coroutine: receives events from the Tauri listen callbacks
    // and performs all signal writes inside the reactive runtime.
    let dispatcher = use_coroutine(
        move |mut rx: UnboundedReceiver<PluginBusEvent>| async move {
            let mut plugin_store = plugin_store;
            let mut toast_store = toast_store;
            while let Ok(event) = rx.recv().await {
                // Toasts need title/message from the payload; capture them
                // before moving the event into the store.
                let toast_info = match &event {
                    PluginBusEvent::Registered { name, .. } => {
                        Some((ToastType::Success, "Plugin Connected".to_string(), format!("{} is now connected", name), 3000))
                    }
                    PluginBusEvent::Error { id, error } => {
                        Some((ToastType::Error, "Plugin Error".to_string(), format!("{}: {}", id, error), 5000))
                    }
                    _ => None,
                };
                plugin_store.write().apply_plugin_event(&event);
                if let Some((toast_type, title, message, duration_ms)) = toast_info {
                    let toast = Toast {
                        id: format!(
                            "toast-plugin-{}",
                            chrono::Utc::now().timestamp_millis()
                        ),
                        toast_type,
                        title,
                        message,
                        duration_ms,
                    };
                    toast_store.write().push(toast);
                }
            }
        },
    );

    let unlistens_effect = unlistens.clone();
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        for channel in [
            "plugin:registryUpdated",
            "plugin:registered",
            "plugin:enabled",
            "plugin:disabled",
            "plugin:error",
            "plugin:event",
        ] {
            let channel_unlistens = unlistens_effect.clone();
            if let Ok(u) = tauri_bridge::listen(channel, move |payload: String| {
                if let Some(event) = parse_plugin_bus_event(channel, &payload) {
                    dispatcher.send(event);
                }
            }) {
                channel_unlistens.borrow_mut().push(u);
            }
        }
    });

    let unlistens_drop = unlistens.clone();
    use_drop(move || {
        let handles = unlistens_drop.borrow_mut().drain(..).collect::<Vec<_>>();
        for handle in handles {
            handle();
        }
    });

    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape emitted by crates/athena-plugins `lifecycle.rs::register_plugin`.
    const REGISTERED_WIRE: &str = r#"{"pluginId":"p1","name":"Test Plugin"}"#;

    #[test]
    fn parses_registered_with_plugin_id_key() {
        let event = parse_plugin_bus_event("plugin:registered", REGISTERED_WIRE)
            .expect("backend-emitted shape must parse");
        let PluginBusEvent::Registered { id, name, version } = event else {
            panic!("expected Registered, got {event:?}");
        };
        assert_eq!(id, "p1");
        assert_eq!(name, "Test Plugin");
        assert_eq!(version, "");
    }

    /// Regression: the previously broken key. The backend emits `pluginId`;
    /// a parser keyed on `id` silently drops every registered/enabled/
    /// disabled/error event. Pin the emitted key against the parsed key.
    #[test]
    fn id_keyed_payload_does_not_masquerade_as_plugin_id() {
        assert!(parse_plugin_bus_event(
            "plugin:registered",
            r#"{"id":"wrong","name":"Wrong"}"#
        )
        .is_none());
        assert!(parse_plugin_bus_event("plugin:enabled", r#"{"id":"p1"}"#).is_none());
        assert!(parse_plugin_bus_event("plugin:disabled", r#"{"id":"p1"}"#).is_none());
        assert!(parse_plugin_bus_event("plugin:error", r#"{"id":"p1","error":"boom"}"#)
            .is_none());
    }

    #[test]
    fn parses_enabled_disabled_and_error_with_plugin_id_key() {
        assert!(matches!(
            parse_plugin_bus_event("plugin:enabled", r#"{"pluginId":"p1","name":"P"}"#),
            Some(PluginBusEvent::Enabled { id }) if id == "p1"
        ));
        assert!(matches!(
            parse_plugin_bus_event("plugin:disabled", r#"{"pluginId":"p1"}"#),
            Some(PluginBusEvent::Disabled { id }) if id == "p1"
        ));
        assert!(matches!(
            parse_plugin_bus_event(
                "plugin:error",
                r#"{"pluginId":"p1","error":"crashed"}"#
            ),
            Some(PluginBusEvent::Error { id, error }) if id == "p1" && error == "crashed"
        ));
    }

    /// Shape emitted by `lifecycle.rs::emit_registry_update`: the plugin
    /// array is wrapped in `{ "registry": [...] }` and each entry carries a
    /// snake_case `status` string (not an `enabled` boolean).
    const REGISTRY_WIRE: &str = r#"{
        "registry": [
            {
                "id": "p1",
                "name": "Enabled One",
                "version": "1.0.0",
                "description": "d",
                "author": "a",
                "status": "enabled",
                "config": {},
                "error": null
            },
            {
                "id": "p2",
                "name": "Broken One",
                "version": "0.2.0",
                "description": "d",
                "author": "a",
                "status": "error",
                "config": {},
                "error": "failed to start"
            }
        ]
    }"#;

    #[test]
    fn parses_registry_updated_wrapped_in_registry_key() {
        let event = parse_plugin_bus_event("plugin:registryUpdated", REGISTRY_WIRE)
            .expect("backend-emitted shape must parse");
        let PluginBusEvent::RegistryUpdated(entries) = event else {
            panic!("expected RegistryUpdated, got {event:?}");
        };
        assert_eq!(entries.len(), 2);
        assert!(entries[0].enabled);
        assert_eq!(entries[0].id, "p1");
        assert!(!entries[1].enabled);
        assert_eq!(entries[1].error.as_deref(), Some("failed to start"));
    }

    #[test]
    fn bare_registry_array_does_not_masquerade_as_wrapped() {
        // The old parser expected a top-level array; the backend wraps it.
        assert!(parse_plugin_bus_event("plugin:registryUpdated", "[]").is_none());
    }

    #[test]
    fn parsed_events_drive_store_updates() {
        let mut store = PluginBusState::new();

        // Registry update installs enabled/error entries.
        store.apply_plugin_event(
            &parse_plugin_bus_event("plugin:registryUpdated", REGISTRY_WIRE).unwrap(),
        );
        assert_eq!(store.plugins.len(), 2);
        assert!(store.plugins.iter().any(|p| p.id == "p1" && p.enabled));
        assert_eq!(
            store.plugins.iter().find(|p| p.id == "p2").unwrap().error.as_deref(),
            Some("failed to start")
        );

        store.apply_plugin_event(
            &parse_plugin_bus_event("plugin:registered", REGISTERED_WIRE).unwrap(),
        );
        assert!(store.plugins.iter().any(|p| p.id == "p1" && p.name == "Test Plugin"));

        store.apply_plugin_event(
            &parse_plugin_bus_event("plugin:disabled", r#"{"pluginId":"p1"}"#).unwrap(),
        );
        assert!(!store.plugins.iter().find(|p| p.id == "p1").unwrap().enabled);

        store.apply_plugin_event(
            &parse_plugin_bus_event("plugin:error", r#"{"pluginId":"p1","error":"boom"}"#)
                .unwrap(),
        );
        assert_eq!(
            store.plugins.iter().find(|p| p.id == "p1").unwrap().error.as_deref(),
            Some("boom")
        );

        store.apply_plugin_event(
            &parse_plugin_bus_event("plugin:event", r#"{"id":"evt-1","type":"status"}"#)
                .unwrap(),
        );
        assert_eq!(store.events.len(), 1);
    }

    #[test]
    fn malformed_payloads_are_dropped_not_panicked() {
        assert!(parse_plugin_bus_event("plugin:registered", "not json").is_none());
        assert!(parse_plugin_bus_event("plugin:registryUpdated", "{}").is_none());
        assert!(parse_plugin_bus_event("plugin:unknown", "{}").is_none());
    }
}
