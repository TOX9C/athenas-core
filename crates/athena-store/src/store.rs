use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Items not found")]
    NotFound(String),
    #[error("{0}")]
    Generic(String),
}

/// Simple key-value JSON store backed by a file in the user's data directory.
/// Enforces immutability of data by returning new values rather than mutating in place.
/// Thread-safe via `Mutex` on the data map (using parking_lot for no poisoning).
pub struct KeyValueStore {
    path: PathBuf,
    data: parking_lot::Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

impl KeyValueStore {
    /// Create or load a store at `~/.config/athena-core`.
    /// (Use full app path derived from `dirs::data_dir()` in production.)
    pub fn new() -> Result<Self, StoreError> {
        Self::with_name_sync("store")
    }

    /// Empty fallback constructor — creates an in-memory store with no persistence.
    /// Used when the real data directory is inaccessible at startup.
    pub fn new_empty() -> Self {
        let data_dir = std::env::temp_dir().join("athena-core-fallback");
        let _ = std::fs::create_dir_all(&data_dir);
        let path = data_dir.join("store.json");
        Self {
            path,
            data: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Synchronous version for initialization (blocking is acceptable at startup).
    pub fn with_name_sync(name: &str) -> Result<Self, StoreError> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("athena-core");
        std::fs::create_dir_all(&data_dir)?;
        let path = data_dir.join(format!("{name}.json"));
        let data: std::collections::HashMap<String, serde_json::Value> = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            std::collections::HashMap::new()
        };
        Ok(Self {
            path,
            data: parking_lot::Mutex::new(data),
        })
    }

    /// Async version for runtime creation/loading.
    pub async fn with_name(name: &str) -> Result<Self, StoreError> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("athena-core");
        let data_dir_clone = data_dir.clone();
        tokio::task::spawn_blocking(move || std::fs::create_dir_all(&data_dir_clone))
            .await
            .map_err(|e| StoreError::Generic(e.to_string()))??;
        let path = data_dir.join(format!("{name}.json"));
        let data: std::collections::HashMap<String, serde_json::Value> = if path.exists() {
            let path_clone = path.clone();
            let content = tokio::task::spawn_blocking(move || std::fs::read_to_string(&path_clone))
                .await
                .map_err(|e| StoreError::Generic(e.to_string()))??;
            serde_json::from_str(&content)?
        } else {
            std::collections::HashMap::new()
        };
        Ok(Self {
            path,
            data: parking_lot::Mutex::new(data),
        })
    }

    /// Retrieve the value for a key, returning a new object, or `None` if absent.
    /// Returns `Err` if deserialization fails for a present key.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, StoreError> {
        let map = self.data.lock();
        match map.get(key) {
            None => Ok(None),
            Some(v) => Ok(Some(serde_json::from_value(v.clone()).map_err(|e| {
                StoreError::Generic(format!("deserialization failed: {}", e))
            })?)),
        }
    }

    /// Set the value for a key and persist to disk. The value must be serializable.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        let json_value = serde_json::to_value(value)?;
        {
            let mut map = self.data.lock();
            map.insert(key.to_string(), json_value);
        }
        self.persist().await
    }

    /// Delete a key and persist.
    pub async fn delete(&self, key: &str) -> Result<(), StoreError> {
        {
            let mut map = self.data.lock();
            map.remove(key);
        }
        self.persist().await
    }

    /// Set a key synchronously and persist to disk (blocking I/O).
    pub fn set_sync<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        let json_value = serde_json::to_value(value)?;
        let json = {
            let mut map = self.data.lock();
            map.insert(key.to_string(), json_value);
            serde_json::to_string_pretty(&*map)?
        };
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json.as_bytes())?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Delete a key synchronously and persist to disk (blocking I/O).
    pub fn delete_sync(&self, key: &str) -> Result<(), StoreError> {
        let json = {
            let mut map = self.data.lock();
            map.remove(key);
            serde_json::to_string_pretty(&*map)?
        };
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json.as_bytes())?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Check if a key exists.
    pub fn has(&self, key: &str) -> bool {
        self.data.lock().contains_key(key)
    }

    async fn persist(&self) -> Result<(), StoreError> {
        let json = {
            let map = self.data.lock();
            serde_json::to_string_pretty(&*map)?
        };
        let path_clone = self.path.clone();
        let tmp_path = self.path.with_extension("json.tmp");
        tokio::task::spawn_blocking(move || {
            std::fs::write(&tmp_path, json.as_bytes())?;
            std::fs::rename(&tmp_path, &path_clone)?;
            Ok::<_, StoreError>(())
        })
        .await
        .map_err(|e| StoreError::Generic(e.to_string()))??;
        Ok(())
    }
}

impl Default for KeyValueStore {
    fn default() -> Self {
        Self::with_name_sync("store").unwrap_or_else(|_| Self::new_empty())
    }
}
