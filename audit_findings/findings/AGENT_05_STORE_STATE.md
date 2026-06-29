# Store Crate & Tauri State Management Audit Findings

## Executive Summary

This audit covers the `athena-store` crate (`KeyValueStore`, `SessionStore`, and data types) and the `AppState` Tauri state initialization. The store crate uses file-backed JSON persistence (not SQLite, contrary to the original audit brief). The `AppState` manages 20+ shared services with a mix of `std::sync::Mutex`, `tokio::sync::Mutex`, and unwrapped `Arc<T>` references.

---

## 1. CRITICAL: `new_empty()` Falls Back to Temp Directory Without Persistence

| Field | Value |
|-------|-------|
| **Severity** | Critical |
| **File** | `crates/athena-store/src/store.rs` |
| **Line** | 24-34 |
| **Category** | Data integrity / Error handling |

**Description:**
Both `KeyValueStore::new_empty()` and `SessionStore::new_empty()` fall back to `std::env::temp_dir()`. Data written to these instances is silently discarded across app restarts. The `AppState::new()` constructor uses these fallbacks when the real data directory is inaccessible, but **does not inform the user** that data will not be persisted.

```rust
// crates/athena-store/src/store.rs:24-34
pub fn new_empty() -> Self {
    let data_dir = std::env::temp_dir().join("athena-core-fallback");
    let _ = std::fs::create_dir_all(&data_dir);
    let path = data_dir.join("store.json");
    Self {
        path,
        data: Mutex::new(std::collections::HashMap::new()), // Empty, not loaded from file!
    }
}
```

**Impact:** 
- User data (chat sessions, API keys in KV store, settings) silently vanishes on app restart.
- Multiple invocations may clobber each other's temp data since the path is deterministic (`athena-core-fallback/store.json`).

**Suggested Fix:**
1. Make `new_empty()` truly in-memory (no file path at all) to make the non-persistence obvious.
2. Propagate the fallback state to callers (e.g., return a bool from `AppState::new()`).
3. Surface a toast/notification to the user when running in fallback mode.

---

## 2. HIGH: `KeyValueStore::has()` Silently Recovers from Poisoned Mutex, Potentially Corrupting State

| Field | Value |
|-------|-------|
| **Severity** | High |
| **File** | `crates/athena-store/src/store.rs` |
| **Line** | 144-153 |
| **Category** | Concurrency / Error handling |

**Description:**
The `has()` method attempts recovery from a poisoned `Mutex` by using `unwrap_or_else(|e| e.into_inner())`. While this allows the method to continue, a poisoned `Mutex` indicates a panic occurred in another thread while the lock was held. Recovering silently means the caller never knows the store may be in an inconsistent state.

```rust
// crates/athena-store/src/store.rs:144-153
pub fn has(&self, key: &str) -> bool {
    self.data
        .lock()
        .map_err(|e| {
            eprintln!("lock poisoned in has(): {}", e);
            e
        })
        .unwrap_or_else(|e| e.into_inner()) // Silent recovery
        .contains_key(key)
}
```

**Impact:**
- Other methods (`get`, `set`, `delete`) propagate poison errors as `StoreError::Generic`, but `has()` swallows them.
- If a panic occurred during a `set` operation, `has()` might report a key as present even though the file was never written.
- Inconsistent error handling across the API surface.

**Suggested Fix:**
Make `has()` return a `Result<bool, StoreError>` like the other methods, or at minimum log an error at `error!` level (not just `eprintln!`).

---

## 3. HIGH: `AppState::new()` Double `unwrap_or_else()` Falls Back Infinitely Without Reporting

| Field | Value |
|-------|-------|
| **Severity** | High |
| **File** | `src-tauri/src/state.rs` |
| **Line** | 370-385 |
| **Category** | Error handling / Resource management |

**Description:**
The `AppState::new()` constructor attempts to create `KeyValueStore` and `SessionStore` with fallback cascades that will never fail:

```rust
// src-tauri/src/state.rs:370-385
let store = Arc::new(match athena_store::KeyValueStore::with_name_sync("store") {
    Ok(s) => s,
    Err(e) => {
        log::error!("KeyValueStore init failed, using empty fallback: {e}");
        athena_store::KeyValueStore::with_name_sync("store") // Retry the SAME operation
            .unwrap_or_else(|_| athena_store::KeyValueStore::new_empty())
    }
});
```

The inner `with_name_sync("store")` is **identical** to the outer attempt. If the first call fails (e.g., disk full, permissions), the second will fail for the same reason. The only saving grace is `new_empty()` always succeeds, but:
- **The retry is useless and misleading** — it looks like an attempt to recover but does the same thing.
- The user is not informed that all persistence is now in-memory-only.

**Impact:**
- Misleading code that suggests a recovery attempt when none exists.
- Silent data loss if `dirs::data_dir()` is ever unwritable.

**Suggested Fix:**
Remove the redundant retry and directly fall back to `new_empty()`. Add a notification or return a warning flag.

```rust
let store = Arc::new(match athena_store::KeyValueStore::with_name_sync("store") {
    Ok(s) => s,
    Err(e) => {
        log::error!("KeyValueStore init failed, using empty fallback: {e}");
        athena_store::KeyValueStore::new_empty()
    }
});
```

---

## 4. HIGH: `TauriEventSender::ask_user` Hard-Codes 5-Minute Timeout

| Field | Value |
|-------|-------|
| **Severity** | High |
| **File** | `src-tauri/src/state.rs` |
| **Line** | 169-204 |
| **Category** | Error handling / Logic |

**Description:**
The `ask_user` method blocks for up to 300 seconds waiting for a user response. If the timeout elapses, it returns the string `"error: user response timed out"` — which the caller may treat as a literal user answer.

```rust
// src-tauri/src/state.rs:201-204
match rx.recv_timeout(std::time::Duration::from_secs(300)) {
    Ok(answer) => answer,
    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
        format!("error: user response timed out")
    }
```

**Impact:**
- An agent tool that asks the user for confirmation could receive the literal string `"error: user response timed out"` and act on it.
- No mechanism to interrupt an in-progress LLM call when the user is unavailable.
- 5 minutes is an eternity for a desktop app; the user may have walked away.

**Suggested Fix:**
Return a dedicated error type or sentinel value that callers can distinguish from a real answer. Consider shorter default timeouts with escalation.

---

## 5. MEDIUM: `SessionStore::new_empty()` Creates Persistent Temp Directories With Side Effects

| Field | Value |
|-------|-------|
| **Severity** | Medium |
| **File** | `crates/athena-store/src/session.rs` |
| **Line** | 24-39 |
| **Category** | Resource management / Side effects |

**Description:**
The `new_empty()` constructor calls `std::fs::create_dir_all()` on a temp path as a side effect:

```rust
// crates/athena-store/src/session.rs:24-39
pub fn new_empty() -> Self {
    let base = std::env::temp_dir().join("athena-core-fallback");
    let sessions_dir = base.join("athena-sessions");
    let images_dir = base.join("athena-images");
    let _ = std::fs::create_dir_all(&sessions_dir);
    let _ = std::fs::create_dir_all(&images_dir);
    SessionStore { sessions_dir, images_dir }
}
```

The `let _ =` swallowing of errors means if `create_dir_all` fails (read-only filesystem), the method returns a `SessionStore` pointing at non-existent directories. Writing to it will fail later with confusing errors.

**Impact:**
- Subsequent `create_session` etc. will fail with generic I/O errors that don't indicate the root cause (directory creation failure).
- Temp directories can accumulate — no cleanup is performed.

**Suggested Fix:**
Return a `Result<Self, SessionStoreError>` from `new_empty()` and propagate directory creation errors.

---

## 6. MEDIUM: `SessionStore::save_image` Stores Base64-Decoded Binary, But `load_image` Re-Encodes as Base64 — Wasteful and Lossy Risk

| Field | Value |
|-------|-------|
| **Severity** | Medium |
| **File** | `crates/athena-store/src/session.rs` |
| **Line** | 84-109 |
| **Category** | Performance / Data integrity |

**Description:**
`save_image` decodes base64 to raw bytes and stores them, but the type system allows any base64 input — including invalid base64. The error is caught at decode time.

More importantly, `load_image` reads raw bytes and **re-encodes them as base64** every time. For a large session with many images, this is wasteful and the round-trip may introduce padding differences that affect if the original base64 hash matches.

```rust
// crates/athena-store/src/session.rs:102-109
pub async fn load_image(&self, image_id: &str) -> Result<Option<String>, SessionStoreError> {
    // ... reads raw bytes, then:
    Ok(Some(base64::engine::general_purpose::STANDARD.encode(&buffer)))
}
```

**Impact:**
- CPU overhead re-encoding images on every load.
- If the original base64 had custom padding or line breaks, the re-encoded form may differ.
- No deduplication — if the same image is attached twice, it's stored twice on disk.

**Suggested Fix:**
1. Store the original base64 string if size permits, or store raw bytes but cache the base64 encoding.
2. Consider content-addressed storage (SHA-256 hash as filename) for image deduplication.

---

## 7. MEDIUM: `ChatSession` Uses `Vec` for Messages — O(n) Append, No Message Limit

| Field | Value |
|-------|-------|
| **Severity** | Medium |
| **File** | `crates/athena-store/src/types.rs` |
| **Line** | 51-68 |
| **Category** | Performance / Resource management |

**Description:**
`ChatSession` stores messages in a `Vec<SessionMessage>`:

```rust
// crates/athena-store/src/types.rs:51-68
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<SessionMessage>,
}
```

There is no upper bound on message count or total size. A long-running chat session can grow unbounded, causing:
- Slow JSON serialization/deserialization
- High memory usage when loaded
- Potential OOM if the session file grows to hundreds of MB

**Impact:**
- Long chat sessions become progressively slower.
- JSON parsing of a multi-GB session file could crash the app.

**Suggested Fix:**
1. Implement message pagination or chunking (store messages in batches or use a real database).
2. Add a configurable message limit with automatic archival.
3. Consider `rkyv` or `bincode` for large session storage instead of JSON.

---

## 8. MEDIUM: `KeyValueStore` Persists on Every `set`/`delete` — Write Amplification

| Field | Value |
|-------|-------|
| **Severity** | Medium |
| **File** | `crates/athena-store/src/store.rs` |
| **Line** | 91-110 |
| **Category** | Performance |

**Description:**
Every `set()` or `delete()` writes the entire in-memory HashMap to disk:

```rust
// crates/athena-store/src/store.rs:157-173 (persist)
async fn persist(&self) -> Result<(), StoreError> {
    let json = {
        let map = self.data.lock().map_err(/* ... */)?;
        serde_json::to_string_pretty(&*map)?
    };
    // ... write entire file
}
```

**Impact:**
- Serializing and writing a large HashMap on every key change is O(n) where n = total store size.
- Rapid successive writes (e.g., batch updates) cause write amplification.
- No batching, no debouncing, no WAL.

**Suggested Fix:**
1. Add a `set_batch()` / `delete_batch()` API that defers persistence.
2. Implement an in-memory dirty flag with periodic background flush.
3. Consider `sled` or `rocksdb` for actual key-value needs.

---

## 9. LOW: `KeyValueStore::with_name_sync` Ignores `name` Parameter for Path Construction

| Field | Value |
|-------|-------|
| **Severity** | Low |
| **File** | `crates/athena-store/src/store.rs` |
| **Line** | 39-56 |
| **Category** | Logic / API design |

**Description:**
The `with_name_sync` and `with_name` constructors accept a `name` parameter but construct the path using `format!("{name}.json")`. If `name` contains path separators (e.g., `../../etc/passwd`), the file can be written outside the intended directory.

```rust
// crates/athena-store/src/store.rs:44-45
let path = data_dir.join(format!("{name}.json"));
```

**Impact:**
- Path traversal if the `name` comes from untrusted input.
- In practice, only internal callers use this, so exploitability is low.

**Suggested Fix:**
Sanitize the `name` parameter — reject any that contain path separators or are not alphanumeric.

---

## 10. LOW: `SessionStore::session_path` and `image_path` Are Not Sanitized

| Field | Value |
|-------|-------|
| **Severity** | Low |
| **File** | `crates/athena-store/src/session.rs` |
| **Line** | 55-61 |
| **Category** | Security / Path traversal |

**Description:**
```rust
// crates/athena-store/src/session.rs:55-61
fn session_path(&self, id: &str) -> PathBuf {
    self.sessions_dir.join(format!("{id}.json"))
}
```

If a session `id` ever contained `../` (e.g., from deserialized JSON), files could be written outside the sessions directory.

**Impact:**
- Low in practice since `id` is generated as a UUID internally.
- But if session IDs ever come from user input or external sources, this becomes exploitable.

**Suggested Fix:**
Validate that `id` does not contain path separators before using it in path construction.

---

## 11. LOW: `AppState::set_app_handle()` Spawns MCP Server On Background Thread Without Await or Error Propagation

| Field | Value |
|-------|-------|
| **Severity** | Low |
| **File** | `src-tauri/src/state.rs` |
| **Line** | 488-511 |
| **Category** | Error handling / Resource management |

**Description:**
```rust
// src-tauri/src/state.rs:488-511
std::thread::spawn(move || {
    let rt = match tokio::runtime::Runtime::new() { /* ... */ };
    rt.block_on(async {
        let mut server = mcp_server.lock().await;
        if let Err(e) = server.init(4545) { /* logs error */ }
    });
});
```

**Issues:**
- Uses `std::thread` to spawn a background thread with its own tokio runtime — wasteful.
- The `JoinHandle` is discarded; caller cannot detect if the MCP server failed to start.
- Port 4545 is hard-coded with no check for availability.
- No mechanism to restart the MCP server if it crashes.

**Impact:**
- Silent failures if port 4545 is in use.
- Thread resource leak if the MCP server init fails — the thread exits but the runtime may not be fully dropped.

**Suggested Fix:**
Use the existing tokio runtime (`tokio::spawn`) instead of spawning a new thread with a new runtime. Return a handle or use a oneshot channel to signal startup success/failure.

---

## 12. LOW: `AppState::Default` Calls `Self::new()` Which Does I/O — Violates `Default` Contract

| Field | Value |
|-------|-------|
| **Severity** | Low |
| **File** | `src-tauri/src/state.rs` |
| **Line** | 338-343 |
| **Category** | Logic / API design |

**Description:**
```rust
// src-tauri/src/state.rs:338-343
impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

`Default::default()` is expected to be cheap and infallible. `AppState::new()` performs filesystem I/O (creating directories, reading store files) and can fail (falling back to `new_empty()`).

**Impact:**
- Surprise for callers using `AppState::default()` who expect it to be lightweight.
- Hidden I/O in a trait method that is often called implicitly.

**Suggested Fix:**
Remove the `Default` implementation or make `new()` truly cheap (lazy-initialize stores).

---

## 13. LOW: `wire_swarm_events` Uses `blocking_lock()` Inside a Non-Async Context (Should Be Fine, But Misleading)

| Field | Value |
|-------|-------|
| **Severity** | Low |
| **File** | `src-tauri/src/state.rs` |
| **Line** | 632-656 |
| **Category** | Logic / Concurrency |

**Description:**
```rust
// src-tauri/src/state.rs:632-656
let swarm = match swarm_coordinator.blocking_lock() {
    guard => guard.clone(),
};
```

The `blocking_lock()` is called in a non-async context (a `fn wire_swarm_events(&self)` so it is technically correct. However, this locks the async mutex synchronously, which can deadlock if any async task on the same thread holds the lock. The `match guard => guard.clone()` syntax is also unnecessarily verbose.

**Impact:**
- Low probability of deadlock since this is called during startup before async tasks run.
- But if ever called from an async context, it would panic.

**Suggested Fix:**
Use `tokio::runtime::Handle::try_current()` to check if we're in a runtime; if so, use `block_on` or defer this work to an async task.

---

## 14. LOW: `AppState::new()` Initializes `TauriEventSender` Before `orchestrator`, But `orchestrator` Is Only Used Later

| Field | Value |
|-------|-------|
| **Severity** | Low |
| **File** | `src-tauri/src/state.rs` |
| **Line** | 368-458 |
| **Category** | Logic / Initialization order |

**Description:**
The initialization order in `AppState::new()` is:
1. Create `store`, `session_store`
2. Create `output_buffer`, `plan_manager`, etc.
3. Create `event_sender` (takes `session_manager` clone)
4. Create `tool_executor` (takes `event_sender`)
5. Create `orchestrator` (takes `tool_executor`, `output_buffer`, etc.)
6. Wire MCP server to `tool_executor`

This order is correct, but the `orchestrator` attempts to restore workspace state from the store:

```rust
// src-tauri/src/state.rs:427-458
if let Ok(Some(json)) = store.get::<String>("workspaces") {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
        // ... matches active workspace ID
    }
}
```

This workspace restoration has no error logging if the JSON parse fails. A corrupted `workspaces` key silently does nothing.

**Impact:**
- Corrupted workspace state is silently ignored, user sees blank workspace on next launch.

**Suggested Fix:**
Add logging for JSON parse errors in the workspace restoration block.

---

## 15. VERY LOW: `cleanup_orphaned_images` Uses a File Extension Check That Will Never Match

| Field | Value |
|-------|-------|
| **Severity** | Very Low / Informational |
| **File** | `crates/athena-store/src/session.rs` |
| **Line** | 267-285 |
| **Category** | Logic / Dead code |

**Description:**
In `cleanup_orphaned_images`, the code filters session files by extension:

```rust
// crates/athena-store/src/session.rs:267-274
if let Some(ext) = path.extension() {
    if ext != "json" {
        continue;
    }
} else {
    continue;
}
```

This is correct for sessions but the inner loop for images does not filter by extension at all — it removes any file whose stem is not in `used_image_ids`. The extension check is correct but the code comment about `"bin"` vs `"bin"` is misleading — images are saved with `.bin` extension (`image_path` uses `format!("{opts.name}`).binary")`).

**Impact:**
- None currently, but the inconsistency between session extension checking and image non-checking is confusing.

**Suggested Fix:**
Remove extension checking from session listing (or add it to image listing for consistency). Rely only on valid JSON deserialization as the actual filter.

---

## 16. VERY LOW: `session_tests.rs` Uses Global Mutex for Test Isolation But Doesn't Actually Isolate

| Field | Value |
|-------|-------|
| **Severity** | Very Low |
| **File** | `crates/athena-store/src/session_tests.rs` |
| **Line** | 1-9 |
| **Category** | Logic / Testing |

**Description:**
```rust
// crates/athena-store/src/session_tests.rs:1-9
static SESSION_MUTEX: Mutex<()> = Mutex::new(());
```

The `SESSION_MUTEX` serializes test execution but **does not** isolate the filesystem. All tests write to the same `~/.config/athena-core/athena-sessions/` directory. Tests run in CI or in parallel could interfere with each other and with production data.

The `tests.rs` for `KeyValueStore` at least uses unique names via `unique_name()`, but it still writes to the real data directory.

**Impact:**
- Flaky tests in CI.
- Risk of clobbering real user data if tests run on a dev machine.

**Suggested Fix:**
Use `tempfile::TempDir` (already a dev-dependency) to create isolated directories for each test.

---

## Summary Table

| # | Severity | File | Line | Category | Finding |
|---|----------|------|------|----------|---------|
| 1 | **Critical** | `store.rs` | 24-34 | Data integrity | `new_empty()` falls back to temp dir, data lost on restart |
| 2 | **High** | `store.rs` | 144-153 | Concurrency | `has()` silently recovers from poisoned mutex |
| 3 | **High** | `state.rs` | 370-385 | Error handling | Redundant retry in `AppState::new()` with useless fallback |
| 4 | **High** | `state.rs` | 169-204 | Logic | `ask_user` hard-codes 5-minute timeout, returns error string as answer |
| 5 | **Medium** | `session.rs` | 24-39 | Resource mgmt | `new_empty()` creates temp dirs, swallows errors |
| 6 | **Medium** | `session.rs` | 84-109 | Performance | `save_image` decodes, `load_image` re-encodes — wasteful |
| 7 | **Medium** | `types.rs` | 51-68 | Performance | `ChatSession` uses unbounded `Vec`, no message limit |
| 8 | **Medium** | `store.rs` | 91-110 | Performance | Every `set`/`delete` rewrites entire store |
| 9 | **Low** | `store.rs` | 39-56 | Security | `name` parameter not sanitized for path traversal |
| 10 | **Low** | `session.rs` | 55-61 | Security | `id` not sanitized in path construction |
| 11 | **Low** | `state.rs` | 488-511 | Error handling | MCP server spawned on thread, errors not propagated |
| 12 | **Low** | `state.rs` | 338-343 | API design | `Default` impl does I/O, violates contract |
| 13 | **Low** | `state.rs` | 632-656 | Concurrency | `blocking_lock()` in non-async context is risky |
| 14 | **Low** | `state.rs` | 427-458 | Logic | Workspace restore silently ignores parse errors |
| 15 | **Very Low** | `session.rs` | 267-285 | Logic | Inconsistent extension checking between sessions and images |
| 16 | **Very Low** | `session_tests.rs` | 1-9 | Testing | Global mutex doesn't isolate filesystem writes |

---

*Report generated: 2026-06-09*
*Audited files: store.rs, session.rs, types.rs, session_tests.rs, tests.rs, state.rs*
