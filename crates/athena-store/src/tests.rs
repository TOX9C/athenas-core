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
        store.set("persist_key", &"persist_value".to_string()).await.unwrap();
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
