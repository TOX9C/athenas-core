use crate::components::shared::toast::{use_toast_store, Toast, ToastType};
use crate::stores::notification::{
    add_notification, use_notification_store, NotificationRecord, NotificationType,
};
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

    pub fn upsert_plugin(&mut self, id: String, name: String, version: String, enabled: bool) {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.name = name;
            p.version = version;
            p.enabled = enabled;
        } else {
            self.plugins.push(PluginEntry {
                id,
                name,
                version,
                enabled,
                error: None,
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
    Registered { id: String, name: String, version: String },
    Enabled { id: String },
    Disabled { id: String },
    Error { id: String, error: String },
    AddEvent(String),
}

/// Plugin event bus component — renders nothing, handles IPC events.
#[component]
pub fn PluginEventBus() -> Element {
    let plugin_store = use_plugin_bus_store();
    let toast_store = use_toast_store();
    let notif_store = use_notification_store();
    let mut mounted = use_signal(|| false);

    let unlistens: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    // Dispatcher coroutine: receives events from the Tauri listen callbacks
    // and performs all signal writes inside the reactive runtime.
    let dispatcher = use_coroutine(move |mut rx: UnboundedReceiver<PluginBusEvent>| async move {
        let mut plugin_store = plugin_store;
        let mut toast_store = toast_store;
        let mut notif_store = notif_store;
        while let Some(event) = rx.recv().await {
            match event {
                PluginBusEvent::RegistryUpdated(entries) => {
                    for entry in entries {
                        plugin_store
                            .write()
                            .upsert_plugin(entry.id, entry.name, entry.version, entry.enabled);
                    }
                }
                PluginBusEvent::Registered { id, name, version } => {
                    plugin_store
                        .write()
                        .upsert_plugin(id.clone(), name.clone(), version, true);

                    let toast = Toast {
                        id: format!("toast-plugin-{}", chrono::Utc::now().timestamp_millis()),
                        toast_type: ToastType::Success,
                        title: "Plugin Connected".to_string(),
                        message: format!("{} is now connected", name),
                        duration_ms: 3000,
                    };
                    toast_store.write().push(toast);

                    let notif = NotificationRecord {
                        id: format!("notif-plugin-{}", chrono::Utc::now().timestamp_millis()),
                        r#type: NotificationType::Success,
                        title: "Plugin Connected".to_string(),
                        message: format!("{} is now connected", name),
                        source: "plugin".to_string(),
                        read: false,
                        timestamp: chrono::Utc::now().timestamp(),
                    };
                    add_notification(&mut notif_store, notif);
                }
                PluginBusEvent::Enabled { id } => {
                    plugin_store.write().set_plugin_enabled(&id, true);
                }
                PluginBusEvent::Disabled { id } => {
                    plugin_store.write().set_plugin_enabled(&id, false);
                }
                PluginBusEvent::Error { id, error } => {
                    plugin_store.write().set_plugin_error(&id, error.clone());

                    let toast = Toast {
                        id: format!("toast-error-{}", chrono::Utc::now().timestamp_millis()),
                        toast_type: ToastType::Error,
                        title: "Plugin Error".to_string(),
                        message: format!("{}: {}", id, error),
                        duration_ms: 5000,
                    };
                    toast_store.write().push(toast);
                }
                PluginBusEvent::AddEvent(payload) => {
                    plugin_store.write().add_event(payload);
                }
            }
        }
    });

    let unlistens_effect = unlistens.clone();
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        let dispatcher = dispatcher.clone();
        let registry_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:registryUpdated", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(plugins_arr) = val.as_array() {
                    let mut entries = Vec::with_capacity(plugins_arr.len());
                    for p in plugins_arr {
                        let id = p
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
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
                        let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                        entries.push(PluginEntry {
                            id,
                            name,
                            version,
                            enabled,
                            error: None,
                        });
                    }
                    dispatcher.send(PluginBusEvent::RegistryUpdated(entries));
                }
            }
        }) {
            registry_unlistens.borrow_mut().push(u);
        }

        let dispatcher = dispatcher.clone();
        let registered_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:registered", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
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
                dispatcher.send(PluginBusEvent::Registered { id, name, version });
            }
        }) {
            registered_unlistens.borrow_mut().push(u);
        }

        let dispatcher = dispatcher.clone();
        let enabled_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:enabled", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !id.is_empty() {
                    dispatcher.send(PluginBusEvent::Enabled { id });
                }
            }
        }) {
            enabled_unlistens.borrow_mut().push(u);
        }

        let dispatcher = dispatcher.clone();
        let disabled_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:disabled", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !id.is_empty() {
                    dispatcher.send(PluginBusEvent::Disabled { id });
                }
            }
        }) {
            disabled_unlistens.borrow_mut().push(u);
        }

        let dispatcher = dispatcher.clone();
        let error_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:error", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let error = val
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                dispatcher.send(PluginBusEvent::Error { id, error });
            }
        }) {
            error_unlistens.borrow_mut().push(u);
        }

        let dispatcher = dispatcher.clone();
        let event_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:event", move |payload: String| {
            dispatcher.send(PluginBusEvent::AddEvent(payload));
        }) {
            event_unlistens.borrow_mut().push(u);
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
