use serde::{Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Items not found")]
    NotFound(String),
}

/// Simple key-value JSON store backed by a file in the user's data directory.

/// Enforces immutability of data by returning new values rather than mutating in place.
/// Thread-safe via `Mutex` on the data map.
pub struct KeyValueStore {
    path: PathBuf,
    data: Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

impl KeyValueStore {
    /// Create or load a store at `~/.config/athena-core`.
    /// (Use full app path derived from `dirs::data_dir()` in production.)
    pub fn new() -> Result<Self, StoreError> {
        Self::with_name("store")
    }

    /// Create or load a store file with the given name under the app's data directory.
    pub fn with_name(name: &str) -> Result<Self, StoreError> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("athena-core");
        fs::create_dir_all(&data_dir)?;
        let path = data_dir.join(format!("{name}.json"));
        let data: std::collections::HashMap<String, serde_json::Value> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    /// Retrieve the value for a key returning a new object, or `None` if absent.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let map = self.data.lock().unwrap();
        map.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set the value for a key and persist to disk.  The value must be serializable.
    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        let json_value = serde_json::to_value(value)?;
        {
            let mut map = self.data.lock().unwrap();
            map.insert(key.to_string(), json_value);
        }
        self.persist()
    }

    /// Delete a key and persist.
    pub fn delete(&self, key: &str) -> Result<(), StoreError> {
        {
            let mut map = self.data.lock().unwrap();
            map.remove(key);
        }
        self.persist()
    }

    /// Check if a key exists.
    pub fn has(&self, key: &str) -> bool {
        let map = self.data.lock().unwrap();
        map.contains_key(key)
    }

    fn persist(&self) -> Result<(), StoreError> {
        let map = self.data.lock().unwrap();
        let json = serde_json::to_string_pretty(&*map)?;
        let mut file = fs::File::create(&self.path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
}

impl Default for KeyValueStore {
    fn default() -> Self {
        Self::new().expect("failed to initialize key-value store")
    }
}

use serde::de::DeserializeOwned;
