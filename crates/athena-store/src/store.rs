use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
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
///
/// When `path` is `None` (constructed via `new_empty`), the store is purely
/// in-memory: `set_sync`/`flush_if_dirty`/`Drop` are no-ops for disk and the
/// data is lost on process exit. Use `is_in_memory()` to detect this
/// condition.
///
/// # Durability contract (F13)
///
/// The debounce creates a crash window: mutations accepted since the last
/// flush are in memory only and are lost if the process dies before
/// `flush_if_dirty`/`Drop` runs. The on-disk snapshot itself is durable —
/// `atomic_write` fsyncs the file and the directory entry before the rename
/// replaces the destination — so a crash never yields a torn or partially
/// updated file, only an older one. This is acceptable because the store
/// holds preferences/UI state, not authoritative data. Revisit (WAL or
/// write-through) only if it ever becomes the system of record.
pub struct KeyValueStore {
    path: Option<PathBuf>,
    data: Arc<parking_lot::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
    dirty: Arc<AtomicBool>,
    revision: Arc<AtomicU64>,
    persist_lock: Arc<parking_lot::Mutex<()>>,
}

impl KeyValueStore {
    /// Create or load a store at `~/.config/athena-core`.
    /// (Use full app path derived from `dirs::data_dir()` in production.)
    pub fn new() -> Result<Self, StoreError> {
        Self::with_name_sync("store")
    }

    /// Empty fallback constructor — creates a truly in-memory store with no
    /// persistence. Used when the real data directory is inaccessible at startup.
    /// In-memory writes succeed; nothing is written to disk. `Drop` is a no-op.
    pub fn new_empty() -> Self {
        Self {
            path: None, // truly in-memory, no persistence
            data: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            dirty: Arc::new(AtomicBool::new(false)),
            revision: Arc::new(AtomicU64::new(0)),
            persist_lock: Arc::new(parking_lot::Mutex::new(())),
        }
    }

    /// Returns `true` when this store is the in-memory fallback (constructed via
    /// `new_empty`). Such a store never writes to disk and is reset on process
    /// restart. Callers may want to surface a warning to the user.
    pub fn is_in_memory(&self) -> bool {
        self.path.is_none()
    }

    /// Test-only constructor: open a store at an explicit path. Used by the
    /// debouncing tests to isolate the on-disk file from production data dir.
    #[doc(hidden)]
    pub fn with_path_sync(path: std::path::PathBuf) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data: std::collections::HashMap<String, serde_json::Value> = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            std::collections::HashMap::new()
        };
        Ok(Self {
            path: Some(path),
            data: Arc::new(parking_lot::Mutex::new(data)),
            dirty: Arc::new(AtomicBool::new(false)),
            revision: Arc::new(AtomicU64::new(0)),
            persist_lock: Arc::new(parking_lot::Mutex::new(())),
        })
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
            path: Some(path),
            data: Arc::new(parking_lot::Mutex::new(data)),
            dirty: Arc::new(AtomicBool::new(false)),
            revision: Arc::new(AtomicU64::new(0)),
            persist_lock: Arc::new(parking_lot::Mutex::new(())),
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
            path: Some(path),
            data: Arc::new(parking_lot::Mutex::new(data)),
            dirty: Arc::new(AtomicBool::new(false)),
            revision: Arc::new(AtomicU64::new(0)),
            persist_lock: Arc::new(parking_lot::Mutex::new(())),
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
    /// For an in-memory fallback store, this updates the in-memory map but
    /// `flush_if_dirty` will not write to disk.
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
    ///
    /// For an in-memory fallback store (constructed via `new_empty`), this
    /// updates the in-memory map and returns `Ok(())` without touching disk.
    pub fn set_sync<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        let json_value = serde_json::to_value(value)?;
        let _persist_guard = self.persist_lock.lock();
        {
            let mut map = self.data.lock();
            map.insert(key.to_string(), json_value);
        }
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.dirty.store(true, Ordering::SeqCst);
        if self.path.is_none() {
            self.dirty.store(false, Ordering::SeqCst);
            return Ok(());
        }
        self.dirty.store(true, Ordering::SeqCst);
        let persisted_revision = self.revision.load(Ordering::SeqCst);
        self.persist_snapshot_locked()?;
        if self.revision.load(Ordering::SeqCst) == persisted_revision {
            self.dirty.store(false, Ordering::SeqCst);
        } else {
            self.dirty.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Delete a key synchronously, persist to disk immediately (blocking I/O).
    /// Bypasses the dirty flag.
    ///
    /// For an in-memory fallback store, this removes the key from the in-memory
    /// map and returns `Ok(())` without touching disk.
    pub fn delete_sync(&self, key: &str) -> Result<(), StoreError> {
        let _persist_guard = self.persist_lock.lock();
        {
            let mut map = self.data.lock();
            map.remove(key);
        }
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.dirty.store(true, Ordering::SeqCst);
        if self.path.is_none() {
            self.dirty.store(false, Ordering::SeqCst);
            return Ok(());
        }
        self.dirty.store(true, Ordering::SeqCst);
        let persisted_revision = self.revision.load(Ordering::SeqCst);
        self.persist_snapshot_locked()?;
        if self.revision.load(Ordering::SeqCst) == persisted_revision {
            self.dirty.store(false, Ordering::SeqCst);
        } else {
            self.dirty.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Check if a key exists.
    pub fn has(&self, key: &str) -> bool {
        self.data.lock().contains_key(key)
    }

    /// Returns the on-disk path backing this store, or `None` for an
    /// in-memory fallback store. Exposed primarily for tests; production
    /// callers should not rely on the path layout.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Mark the store as having pending writes. Cheap atomic flag — safe to
    /// call on every mutation.
    fn mark_dirty(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Monotonic mutation counter: bumped on every `set`/`delete`/sync
    /// persist. Callers that poll a hot key (e.g. the 1.5 s heartbeat) can
    /// cache the parsed value and re-parse only when this changes.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Returns `true` if there are pending writes that have not been flushed.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// If the store is dirty, write the in-memory state to disk and clear the
    /// flag. Returns `Ok(())` whether or not a write was required.
    ///
    /// For an in-memory fallback store, this is a no-op (the dirty bit is
    /// cleared but no file is touched).
    ///
    /// Call this after a batch of `set`/`delete` calls to persist them.
    pub async fn flush_if_dirty(&self) -> Result<(), StoreError> {
        if self.path.is_none() {
            // In-memory fallback: no file to flush. Clear the dirty bit so
            // is_dirty() accurately reflects that there is nothing to write.
            self.dirty.store(false, Ordering::SeqCst);
            return Ok(());
        }
        if !self.dirty.load(Ordering::SeqCst) {
            return Ok(());
        }
        let store = self.clone_for_persistence();
        tokio::task::spawn_blocking(move || store.persist_snapshot())
            .await
            .map_err(|e| StoreError::Generic(e.to_string()))??;
        Ok(())
    }

    fn clone_for_persistence(&self) -> PersistenceHandle {
        PersistenceHandle {
            path: self.path.clone(),
            data: Arc::clone(&self.data),
            dirty: Arc::clone(&self.dirty),
            revision: Arc::clone(&self.revision),
            persist_lock: Arc::clone(&self.persist_lock),
        }
    }

    fn persist_snapshot_locked(&self) -> Result<(), StoreError> {
        let path = match &self.path {
            Some(path) => path.clone(),
            None => {
                self.dirty.store(false, Ordering::SeqCst);
                return Ok(());
            }
        };
        let revision = self.revision.load(Ordering::SeqCst);
        let json = {
            let map = self.data.lock();
            // Compact: machine-written, machine-read (values go through
            // get/set APIs); pretty-printing costs ~25% more bytes and a
            // slower serde pass on every persist.
            serde_json::to_string(&*map)?
        };
        atomic_write(&path, json.as_bytes())?;
        if self.revision.load(Ordering::SeqCst) == revision {
            self.dirty.store(false, Ordering::SeqCst);
        } else {
            self.dirty.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
}

struct PersistenceHandle {
    path: Option<PathBuf>,
    data: Arc<parking_lot::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
    dirty: Arc<AtomicBool>,
    revision: Arc<AtomicU64>,
    persist_lock: Arc<parking_lot::Mutex<()>>,
}

impl PersistenceHandle {
    fn persist_snapshot(self) -> Result<(), StoreError> {
        let _persist_guard = self.persist_lock.lock();
        let path = match self.path {
            Some(path) => path,
            None => {
                self.dirty.store(false, Ordering::SeqCst);
                return Ok(());
            }
        };
        let revision = self.revision.load(Ordering::SeqCst);
        let json = {
            let map = self.data.lock();
            serde_json::to_string(&*map)?
        };
        atomic_write(&path, json.as_bytes())?;
        if self.revision.load(Ordering::SeqCst) == revision {
            self.dirty.store(false, Ordering::SeqCst);
        } else {
            self.dirty.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
}

/// Write a complete snapshot to a unique same-directory temporary file, sync
/// the file, atomically replace the destination, and sync the directory entry
/// where supported. The temporary path is removed on failure.
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("store.json"));
    let temp_path = parent.join(format!(".{name}.tmp-{}", uuid::Uuid::new_v4()));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, path)?;
        #[cfg(unix)]
        {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok::<(), std::io::Error>(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result.map_err(StoreError::from)
}

/// On drop, attempt a best-effort synchronous flush if dirty. This protects
/// against losing pending writes when the store is dropped (e.g., on app exit)
/// without requiring every caller to remember to flush.
///
/// For an in-memory fallback store (constructed via `new_empty`), Drop is a
/// no-op: the in-memory data is released by the destructor naturally.
impl Drop for KeyValueStore {
    fn drop(&mut self) {
        // In-memory fallback: nothing to persist.
        if self.path.is_none() {
            return;
        }
        if !self.dirty.load(Ordering::SeqCst) {
            return;
        }
        let handle = self.clone_for_persistence();
        if let Err(e) = handle.persist_snapshot() {
            eprintln!("KeyValueStore drop: durable flush failed: {e}");
        }
    }
}

impl Default for KeyValueStore {
    fn default() -> Self {
        Self::with_name_sync("store").unwrap_or_else(|_| Self::new_empty())
    }
}
