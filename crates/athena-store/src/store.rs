use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
///
/// Writes are debounced: `set`/`delete` mark the store dirty (cheap atomic flag)
/// rather than writing immediately. The actual disk write happens when the caller
/// invokes `flush_if_dirty` (or implicitly on `Drop`). This batches rapid IPC
/// mutations and avoids O(n) write amplification.
pub struct KeyValueStore {
    path: PathBuf,
    data: parking_lot::Mutex<std::collections::HashMap<String, serde_json::Value>>,
    dirty: AtomicBool,
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
            dirty: AtomicBool::new(false),
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
            dirty: AtomicBool::new(false),
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
            dirty: AtomicBool::new(false),
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

    /// Set the value for a key. Marks the store dirty; the actual disk write
    /// is deferred to `flush_if_dirty` (or `Drop`). Use this when emitting many
    /// writes in quick succession to avoid O(n) write amplification.
    ///
    /// Call `flush_if_dirty().await` after a batch of mutations to persist them.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        let json_value = serde_json::to_value(value)?;
        {
            let mut map = self.data.lock();
            map.insert(key.to_string(), json_value);
        }
        self.mark_dirty();
        Ok(())
    }

    /// Delete a key. Marks the store dirty; the actual disk write is deferred
    /// to `flush_if_dirty` (or `Drop`).
    pub async fn delete(&self, key: &str) -> Result<(), StoreError> {
        {
            let mut map = self.data.lock();
            map.remove(key);
        }
        self.mark_dirty();
        Ok(())
    }

    /// Set a key synchronously, persist to disk immediately (blocking I/O).
    /// Bypasses the dirty flag — useful when the caller needs durability
    /// guarantees for a single write without flushing an unrelated batch.
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
        // In-memory state matches disk; clear any pending dirty bit.
        self.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Delete a key synchronously, persist to disk immediately (blocking I/O).
    /// Bypasses the dirty flag.
    pub fn delete_sync(&self, key: &str) -> Result<(), StoreError> {
        let json = {
            let mut map = self.data.lock();
            map.remove(key);
            serde_json::to_string_pretty(&*map)?
        };
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json.as_bytes())?;
        std::fs::rename(&tmp_path, &self.path)?;
        self.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if a key exists.
    pub fn has(&self, key: &str) -> bool {
        self.data.lock().contains_key(key)
    }

    /// Mark the store as having pending writes. Cheap atomic flag — safe to
    /// call on every mutation.
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if there are pending writes that have not been flushed.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Atomically check the dirty flag and clear it. Returns `true` if a flush
    /// is required (caller should invoke `persist`).
    fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    /// If the store is dirty, write the in-memory state to disk and clear the
    /// flag. Returns `Ok(())` whether or not a write was required.
    ///
    /// Call this after a batch of `set`/`delete` calls to persist them.
    pub async fn flush_if_dirty(&self) -> Result<(), StoreError> {
        if !self.take_dirty() {
            return Ok(());
        }
        self.persist().await
    }

    /// Persist the current in-memory state to disk unconditionally.
    /// Used by `flush_if_dirty` and `Drop`; rarely needed by callers.
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

/// On drop, attempt a best-effort synchronous flush if dirty. This protects
/// against losing pending writes when the store is dropped (e.g., on app exit)
/// without requiring every caller to remember to flush.
impl Drop for KeyValueStore {
    fn drop(&mut self) {
        if !self.dirty.load(Ordering::SeqCst) {
            return;
        }
        let json = match {
            let map = self.data.lock();
            serde_json::to_string_pretty(&*map)
        } {
            Ok(j) => j,
            Err(e) => {
                eprintln!("KeyValueStore drop: failed to serialize: {e}");
                return;
            }
        };
        let tmp_path = self.path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp_path, json.as_bytes()) {
            eprintln!("KeyValueStore drop: write failed: {e}");
            return;
        }
        if let Err(e) = std::fs::rename(&tmp_path, &self.path) {
            eprintln!("KeyValueStore drop: rename failed: {e}");
        }
    }
}

impl Default for KeyValueStore {
    fn default() -> Self {
        Self::with_name_sync("store").unwrap_or_else(|_| Self::new_empty())
    }
}
