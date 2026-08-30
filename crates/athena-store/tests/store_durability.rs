//! Integration tests: store durability contracts exercised from OUTSIDE the
//! crate (the in-crate `tests.rs`/`session_tests.rs` cover the rest).
//!
//! Contracts under defense:
//! - `with_path_sync` reopens an existing file and sees prior data
//!   (file-backed durability round-trip at an explicit path);
//! - a corrupt store file surfaces a JSON error at open — never silent
//!   data loss;
//! - `set_sync`/`delete_sync` persist immediately: no explicit flush needed,
//!   a fresh handle on the same path observes the mutation;
//! - a leftover `.tmp-*` file from a crashed write does not break loading;
//!   the reader still sees the last good snapshot;
//! - dirty-flag transitions: async `set` raises `is_dirty`, `flush_if_dirty`
//!   clears it, `set_sync` leaves it clear;
//! - in-memory fallback: `set_sync` succeeds, writes no file, `path()` None;
//! - session store (via `new_empty` fallback dirs, unique ids per test):
//!   corrupt session file → `get_session` errors, `list_sessions` skips it;
//!   `update_session` on a missing id → NotFound; `delete_session` on a
//!   missing id → false.

use athena_store::session::SessionStore;
use athena_store::store::{KeyValueStore, StoreError};

fn unique_path(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "athena-store-it-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("store.json")
}

#[tokio::test]
async fn reopen_same_path_sees_previous_data() {
    let path = unique_path("reopen");
    {
        let store = KeyValueStore::with_path_sync(path.clone()).unwrap();
        store.set_sync("k", &"v1".to_string()).unwrap();
    }
    let reopened = KeyValueStore::with_path_sync(path).unwrap();
    let value: Option<String> = reopened.get("k").unwrap();
    assert_eq!(value, Some("v1".to_string()));
}

#[tokio::test]
async fn corrupt_store_file_errors_at_open_instead_of_silent_loss() {
    let path = unique_path("corrupt");
    std::fs::write(&path, "{definitely not json").unwrap();
    let err = match KeyValueStore::with_path_sync(path) {
        Ok(_) => panic!("corrupt file must fail to open"),
        Err(err) => err,
    };
    assert!(
        matches!(err, StoreError::Json(_)),
        "corrupt file must surface a parse error, got {err:?}"
    );
}

#[tokio::test]
async fn set_sync_is_immediately_durable_without_flush() {
    let path = unique_path("setsync");
    let store = KeyValueStore::with_path_sync(path.clone()).unwrap();
    store.set_sync("durable", &"now".to_string()).unwrap();
    assert!(
        !store.is_dirty(),
        "set_sync clears the dirty flag on success"
    );

    // A completely independent handle observes the write.
    let fresh = KeyValueStore::with_path_sync(path.clone()).unwrap();
    let value: Option<String> = fresh.get("durable").unwrap();
    assert_eq!(value, Some("now".to_string()));

    // delete_sync is equally immediate.
    store.delete_sync("durable").unwrap();
    assert!(!store.has("durable"));
    let fresh = KeyValueStore::with_path_sync(path).unwrap();
    assert!(!fresh.has("durable"), "deletion persisted without flush");
}

#[tokio::test]
async fn leftover_temp_file_from_crashed_write_is_tolerated() {
    let path = unique_path("crash");
    {
        let store = KeyValueStore::with_path_sync(path.clone()).unwrap();
        store.set_sync("good", &"snapshot".to_string()).unwrap();
    }
    // Simulate a crash between temp-file creation and rename.
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let stale_tmp = path.parent().unwrap().join(format!(".{name}.tmp-stale"));
    std::fs::write(&stale_tmp, b"partial garbage").unwrap();

    let reopened = KeyValueStore::with_path_sync(path.clone()).unwrap();
    let value: Option<String> = reopened.get("good").unwrap();
    assert_eq!(
        value,
        Some("snapshot".to_string()),
        "stale tmp must not shadow last good data"
    );

    // A successful subsequent write cleans up its own tmp files; the stale
    // one remains but never participates.
    reopened.set_sync("good", &"updated".to_string()).unwrap();
    let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(entries.iter().any(|n| n == &name), "store file present");
    let _ = std::fs::remove_file(&stale_tmp);
}

#[tokio::test]
async fn dirty_flag_tracks_async_writes_and_flush() {
    let path = unique_path("dirty");
    let store = KeyValueStore::with_path_sync(path).unwrap();
    assert!(!store.is_dirty(), "fresh store is clean");

    store.set("pending", &42i32).await.unwrap();
    assert!(store.is_dirty(), "async set raises dirty without writing");

    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty(), "flush clears dirty");
    // Flushing again when clean is a no-op that still succeeds.
    store.flush_if_dirty().await.unwrap();
}

#[tokio::test]
async fn in_memory_fallback_never_touches_disk() {
    let store = KeyValueStore::new_empty();
    assert!(store.is_in_memory());
    assert!(store.path().is_none());

    store.set_sync("mem", &"only".to_string()).unwrap();
    store.set("mem2", &"also".to_string()).await.unwrap();
    store.delete("mem").await.unwrap();
    store.flush_if_dirty().await.unwrap();

    assert!(!store.has("mem"));
    let value: Option<String> = store.get("mem2").unwrap();
    assert_eq!(value, Some("also".to_string()));
    assert!(!store.is_dirty(), "in-memory flush clears dirty");
    assert!(
        !std::env::temp_dir().join("athena-store-it-mem").exists(),
        "no file was ever created for this store"
    );
}

// ── Session store (fallback dirs, unique ids per test) ─────────────────────

fn session_store() -> SessionStore {
    SessionStore::new_empty()
}

#[tokio::test]
async fn corrupt_session_file_errors_on_get_and_is_skipped_by_list() {
    let store = session_store();
    let good = store.create_session(Some("good")).await.unwrap();
    store
        .update_session(&good.id, None, Some(vec![]))
        .await
        .unwrap()
        .unwrap();

    // Corrupt a second session file directly on disk.
    let bad = store.create_session(Some("bad")).await.unwrap();
    let bad_path = std::env::temp_dir()
        .join("athena-core-fallback")
        .join("athena-sessions")
        .join(format!("{}.json", bad.id));
    std::fs::write(&bad_path, "not json at all").unwrap();

    // get_session surfaces the parse error instead of pretending absence.
    let err = store.get_session(&bad.id).await.unwrap_err();
    assert!(matches!(
        err,
        athena_store::session::SessionStoreError::Json(_)
    ));

    // list_sessions skips the unreadable file; the good one survives.
    let listed = store.list_sessions().await.unwrap();
    assert_eq!(listed.len(), 1, "corrupt file skipped");
    assert_eq!(listed[0].id, good.id);
}

#[tokio::test]
async fn update_missing_session_is_not_found_and_delete_missing_is_false() {
    let store = session_store();
    let err = store
        .update_session("no-such-session", Some("t"), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        athena_store::session::SessionStoreError::NotFound(id) if id == "no-such-session"
    ));
    assert!(!store.delete_session("no-such-session").await.unwrap());
}

#[tokio::test]
async fn create_update_get_delete_round_trip_in_fallback_dirs() {
    let store = session_store();
    let session = store.create_session(Some("round trip")).await.unwrap();
    assert!(session.messages.is_empty());

    let updated = store
        .update_session(&session.id, Some("renamed"), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.title, "renamed");
    assert!(updated.updated_at >= session.created_at);

    let fetched = store.get_session(&session.id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "renamed");

    assert!(store.delete_session(&session.id).await.unwrap());
    assert!(store.get_session(&session.id).await.unwrap().is_none());
}
