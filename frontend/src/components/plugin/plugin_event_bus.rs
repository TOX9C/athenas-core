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

/// Plugin event bus component — renders nothing, handles IPC events.
#[component]
pub fn PluginEventBus() -> Element {
    let plugin_store = use_plugin_bus_store();
    let toast_store = use_toast_store();
    let notif_store = use_notification_store();
    let mut mounted = use_signal(|| false);

    let unlistens: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
        use_hook(|| Rc::new(RefCell::new(Vec::new())));

    let unlistens_effect = unlistens.clone();
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        let mut registry_store = plugin_store;
        let registry_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:registryUpdated", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(plugins_arr) = val.as_array() {
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
                        registry_store
                            .write()
                            .upsert_plugin(id, name, version, enabled);
                    }
                }
            }
        }) {
            registry_unlistens.borrow_mut().push(u);
        }

        let mut registered_store = plugin_store;
        let mut registered_toast = toast_store;
        let mut registered_notif = notif_store;
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
                registered_store
                    .write()
                    .upsert_plugin(id.clone(), name.clone(), version, true);

                let toast = Toast {
                    id: format!("toast-plugin-{}", chrono::Utc::now().timestamp_millis()),
                    toast_type: ToastType::Success,
                    title: "Plugin Connected".to_string(),
                    message: format!("{} is now connected", name),
                    duration_ms: 3000,
                };
                registered_toast.write().push(toast);

                let notif = NotificationRecord {
                    id: format!("notif-plugin-{}", chrono::Utc::now().timestamp_millis()),
                    r#type: NotificationType::Success,
                    title: "Plugin Connected".to_string(),
                    message: format!("{} is now connected", name),
                    source: "plugin".to_string(),
                    read: false,
                    timestamp: chrono::Utc::now().timestamp(),
                };
                add_notification(&mut registered_notif, notif);
            }
        }) {
            registered_unlistens.borrow_mut().push(u);
        }

        let mut enabled_store = plugin_store;
        let enabled_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:enabled", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() {
                    enabled_store.write().set_plugin_enabled(id, true);
                }
            }
        }) {
            enabled_unlistens.borrow_mut().push(u);
        }

        let mut disabled_store = plugin_store;
        let disabled_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:disabled", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() {
                    disabled_store.write().set_plugin_enabled(id, false);
                }
            }
        }) {
            disabled_unlistens.borrow_mut().push(u);
        }

        let mut error_store = plugin_store;
        let mut error_toast = toast_store;
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
                error_store.write().set_plugin_error(&id, error.clone());

                let toast = Toast {
                    id: format!("toast-error-{}", chrono::Utc::now().timestamp_millis()),
                    toast_type: ToastType::Error,
                    title: "Plugin Error".to_string(),
                    message: format!("{}: {}", id, error),
                    duration_ms: 5000,
                };
                error_toast.write().push(toast);
            }
        }) {
            error_unlistens.borrow_mut().push(u);
        }

        let mut event_store = plugin_store;
        let event_unlistens = unlistens_effect.clone();
        if let Ok(u) = tauri_bridge::listen("plugin:event", move |payload: String| {
            event_store.write().add_event(payload.clone());
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
