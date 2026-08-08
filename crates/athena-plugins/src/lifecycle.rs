//! Plugin registration, lifecycle, listing, and configuration operations.

use super::{
    build_registry_info, now_millis, validate_plugin_manifest, PluginEntry, PluginError,
    PluginInfo, PluginManager, PluginManifest, PluginStatus,
};
use std::collections::HashMap;

impl PluginManager {
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

    pub fn get_plugin(&self, plugin_id: &str) -> Option<PluginEntry> {
        let inner = self.inner.lock().ok()?;
        inner.plugins.get(plugin_id).cloned()
    }

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

    pub fn get_plugin_config(&self, plugin_id: &str) -> Option<serde_json::Value> {
        let inner = self.inner.lock().ok()?;
        inner.plugins.get(plugin_id).map(|e| e.config.clone())
    }

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
