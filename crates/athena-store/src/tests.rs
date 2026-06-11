use crate::KeyValueStore;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_name() -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("test_store_{}", n)
}

#[tokio::test]
async fn test_set_and_get() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    store.set("key1", &"value1".to_string()).await.unwrap();
    let result: Option<String> = store.get("key1").unwrap();
    assert_eq!(result, Some("value1".to_string()));
}

#[tokio::test]
async fn test_overwrite() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    store.set("key1", &"value1".to_string()).await.unwrap();
    store.set("key1", &"value2".to_string()).await.unwrap();
    let result: Option<String> = store.get("key1").unwrap();
    assert_eq!(result, Some("value2".to_string()));
}

#[tokio::test]
async fn test_get_nonexistent() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    let result: Option<String> = store.get("nonexistent").unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn test_delete() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    store.set("key1", &"value1".to_string()).await.unwrap();
    store.delete("key1").await.unwrap();
    let result: Option<String> = store.get("key1").unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn test_has() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    store.set("key1", &"value1".to_string()).await.unwrap();
    assert!(store.has("key1"));
    assert!(!store.has("nonexistent"));
}

#[tokio::test]
async fn test_persistence() {
    let name = unique_name();
    {
        let store = KeyValueStore::with_name_sync(&name).unwrap();
        store
            .set("persist_key", &"persist_value".to_string())
            .await
            .unwrap();
    }
    {
        let store2 = KeyValueStore::with_name_sync(&name).unwrap();
        let result: Option<String> = store2.get("persist_key").unwrap();
        assert_eq!(result, Some("persist_value".to_string()));
    }
}

#[tokio::test]
async fn test_multiple_keys() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    for i in 0..10 {
        store
            .set(&format!("key_{}", i), &format!("value_{}", i).to_string())
            .await
            .unwrap();
    }
    for i in 0..10 {
        let result: Option<String> = store.get(&format!("key_{}", i)).unwrap();
        assert_eq!(result, Some(format!("value_{}", i)));
    }
}

#[tokio::test]
async fn test_empty_value() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    store.set("empty", &"".to_string()).await.unwrap();
    let result: Option<String> = store.get("empty").unwrap();
    assert_eq!(result, Some("".to_string()));
}

#[tokio::test]
async fn test_special_characters() {
    let store = KeyValueStore::with_name_sync(&unique_name()).unwrap();
    let key = "key_🔑_αβγ";
    let value = "value_✨_日本語_ñöäü";
    store.set(key, &value.to_string()).await.unwrap();
    let result: Option<String> = store.get(key).unwrap();
    assert_eq!(result, Some(value.to_string()));
}

// --- Dirty/flush debouncing tests (Task 1.7) ---

#[tokio::test]
async fn test_set_marks_dirty_but_does_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    let store = KeyValueStore::with_path_sync(path.clone()).unwrap();

    // Seed: write a value, flush, then capture on-disk bytes.
    store.set("seed", &"x".to_string()).await.unwrap();
    store.flush_if_dirty().await.unwrap();
    let bytes_before = std::fs::read(&path).unwrap();

    // Now set many keys WITHOUT flushing. Dirty flag rises, file untouched.
    for i in 0..10 {
        store
            .set(&format!("k{i}"), &format!("v{i}").to_string())
            .await
            .unwrap();
    }
    assert!(store.is_dirty());
    let bytes_after_set = std::fs::read(&path).unwrap();
    assert_eq!(
        bytes_before, bytes_after_set,
        "set() must not rewrite the file when flush_if_dirty is not called"
    );

    // After flush, on-disk content includes the new keys.
    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty());
    let bytes_after_flush = std::fs::read(&path).unwrap();
    assert_ne!(
        bytes_before, bytes_after_flush,
        "flush_if_dirty() must rewrite the file when dirty"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&bytes_after_flush).unwrap();
    assert!(parsed.get("k0").is_some());
    assert!(parsed.get("k9").is_some());
}

#[tokio::test]
async fn test_flush_if_dirty_clears_dirty_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    let store = KeyValueStore::with_path_sync(path).unwrap();

    store.set("a", &"b".to_string()).await.unwrap();
    assert!(store.is_dirty());
    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty());
    // Second flush is a no-op and does not error.
    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty());
    // Data is still there.
    let v: Option<String> = store.get("a").unwrap();
    assert_eq!(v, Some("b".to_string()));
}

#[tokio::test]
async fn test_burst_of_writes_persists_via_flush() {
    let name = unique_name();
    let store = KeyValueStore::with_name_sync(&name).unwrap();
    for i in 0..100 {
        store
            .set(&format!("k{i}"), &format!("v{i}").to_string())
            .await
            .unwrap();
    }
    // 100 sets coalesced into a single flush.
    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty());
    // Re-open and verify all 100 keys.
    drop(store);
    let store2 = KeyValueStore::with_name_sync(&name).unwrap();
    for i in 0..100 {
        let v: Option<String> = store2.get(&format!("k{i}")).unwrap();
        assert_eq!(v, Some(format!("v{i}")));
    }
}

#[tokio::test]
async fn test_drop_flushes_pending_writes() {
    let name = unique_name();
    {
        let store = KeyValueStore::with_name_sync(&name).unwrap();
        store.set("drop_key", &"drop_value".to_string()).await.unwrap();
        assert!(store.is_dirty());
        // Intentionally NO explicit flush — Drop should persist.
    }
    let store2 = KeyValueStore::with_name_sync(&name).unwrap();
    let v: Option<String> = store2.get("drop_key").unwrap();
    assert_eq!(v, Some("drop_value".to_string()));
}

#[tokio::test]
async fn test_is_dirty_false_for_fresh_store() {
    let name = unique_name();
    let store = KeyValueStore::with_name_sync(&name).unwrap();
    assert!(!store.is_dirty());
}

// --- In-memory fallback tests (Task 3.5) ---

#[test]
fn test_new_empty_is_in_memory_and_has_no_path() {
    let store = KeyValueStore::new_empty();
    assert!(store.is_in_memory(), "new_empty must report in-memory mode");
    assert!(store.path().is_none(), "new_empty must have no on-disk path");
    assert!(!store.is_dirty(), "fresh in-memory store is not dirty");
}

#[tokio::test]
async fn test_new_empty_does_not_persist_to_disk() {
    // Snapshot the fallback temp dir state *before* — `new_empty` must not
    // create a new file in it (and we want to prove no `store.json` appears).
    let fallback_dir = std::env::temp_dir().join("athena-core-fallback");
    let store = KeyValueStore::new_empty();
    let _ = std::fs::remove_dir_all(&fallback_dir); // clean slate

    store.set("k", &"v".to_string()).await.unwrap();
    assert!(store.is_dirty());
    // flush_if_dirty on an in-memory store must be a no-op, not an error.
    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty(), "flush_if_dirty should clear dirty bit");
    // No file should have been created in the temp fallback dir.
    assert!(
        !fallback_dir.exists(),
        "new_empty must not write to the temp fallback dir"
    );

    // set_sync must succeed (in-memory only) and also not touch disk.
    store.set_sync("k2", &"v2".to_string()).unwrap();
    assert!(
        !fallback_dir.exists(),
        "set_sync on an in-memory store must not create a temp fallback dir"
    );

    // In-memory read-after-write should still work.
    let v1: Option<String> = store.get("k").unwrap();
    assert_eq!(v1, Some("v".to_string()));
    let v2: Option<String> = store.get("k2").unwrap();
    assert_eq!(v2, Some("v2".to_string()));
}

#[tokio::test]
async fn test_new_empty_drop_does_not_write() {
    // Repeatable: create, mutate, drop — must not produce a file.
    let fallback_dir = std::env::temp_dir().join("athena-core-fallback");
    let _ = std::fs::remove_dir_all(&fallback_dir);

    {
        let store = KeyValueStore::new_empty();
        store.set("drop_key", &"drop_value".to_string()).await.unwrap();
        assert!(store.is_dirty());
        // Intentionally NO explicit flush — Drop must not write to disk.
    }

    assert!(
        !fallback_dir.exists(),
        "Drop of an in-memory store must not create a temp fallback dir"
    );
}

#[test]
fn test_persistent_store_is_not_in_memory() {
    let name = unique_name();
    let store = KeyValueStore::with_name_sync(&name).unwrap();
    assert!(
        !store.is_in_memory(),
        "stores opened with a real path must report non-fallback"
    );
    assert!(store.path().is_some(), "real store must have an on-disk path");
}

#[tokio::test]
async fn test_new_empty_delete_sync_is_noop() {
    let store = KeyValueStore::new_empty();
    store.set("k", &"v".to_string()).await.unwrap();
    // delete_sync on an in-memory store should succeed without touching disk.
    store.delete_sync("k").unwrap();
    let v: Option<String> = store.get("k").unwrap();
    assert_eq!(v, None, "in-memory delete_sync removes the in-memory key");
}

// --- In-memory fallback tests (Task 3.5) ---

#[test]
fn test_new_empty_is_in_memory_and_has_no_path() {
    let store = KeyValueStore::new_empty();
    assert!(store.is_in_memory(), "new_empty must report in-memory mode");
    assert!(store.path().is_none(), "new_empty must have no on-disk path");
    assert!(!store.is_dirty(), "fresh in-memory store is not dirty");
}

#[tokio::test]
async fn test_new_empty_does_not_persist_to_disk() {
    // Snapshot the fallback temp dir state *before* — `new_empty` must not
    // create a new file in it (and we want to prove no `store.json` appears).
    let fallback_dir = std::env::temp_dir().join("athena-core-fallback");
    let store = KeyValueStore::new_empty();
    let _ = std::fs::remove_dir_all(&fallback_dir); // clean slate

    store.set("k", &"v".to_string()).await.unwrap();
    assert!(store.is_dirty());
    // flush_if_dirty on an in-memory store must be a no-op, not an error.
    store.flush_if_dirty().await.unwrap();
    assert!(!store.is_dirty(), "flush_if_dirty should clear dirty bit");
    // No file should have been created in the temp fallback dir.
    assert!(
        !fallback_dir.exists(),
        "new_empty must not write to the temp fallback dir"
    );

    // set_sync must succeed (in-memory only) and also not touch disk.
    store.set_sync("k2", &"v2".to_string()).unwrap();
    assert!(
        !fallback_dir.exists(),
        "set_sync on an in-memory store must not create a temp fallback dir"
    );

    // In-memory read-after-write should still work.
    let v1: Option<String> = store.get("k").unwrap();
    assert_eq!(v1, Some("v".to_string()));
    let v2: Option<String> = store.get("k2").unwrap();
    assert_eq!(v2, Some("v2".to_string()));
}

#[tokio::test]
async fn test_new_empty_drop_does_not_write() {
    // Repeatable: create, mutate, drop — must not produce a file.
    let fallback_dir = std::env::temp_dir().join("athena-core-fallback");
    let _ = std::fs::remove_dir_all(&fallback_dir);

    {
        let store = KeyValueStore::new_empty();
        store.set("drop_key", &"drop_value".to_string()).await.unwrap();
        assert!(store.is_dirty());
        // Intentionally NO explicit flush — Drop must not write to disk.
    }

    assert!(
        !fallback_dir.exists(),
        "Drop of an in-memory store must not create a temp fallback dir"
    );
}

#[test]
fn test_persistent_store_is_not_in_memory() {
    let name = unique_name();
    let store = KeyValueStore::with_name_sync(&name).unwrap();
    assert!(
        !store.is_in_memory(),
        "stores opened with a real path must report non-fallback"
    );
    assert!(store.path().is_some(), "real store must have an on-disk path");
}

#[tokio::test]
async fn test_new_empty_delete_sync_is_noop() {
    let store = KeyValueStore::new_empty();
    store.set("k", &"v".to_string()).await.unwrap();
    // delete_sync on an in-memory store should succeed without touching disk.
    store.delete_sync("k").unwrap();
    let v: Option<String> = store.get("k").unwrap();
    assert_eq!(v, None, "in-memory delete_sync removes the in-memory key");
}
