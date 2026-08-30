use crate::session::{SessionStore, SessionStoreError};
use crate::types::{ImageRef, MessageRole, SessionMessage};
use base64::Engine;

/// Build a SessionStore rooted in a fresh temp directory.  Each test gets
/// isolated dirs so file counts are deterministic and no test touches the
/// user's real `~/Library/Application Support/athena-core` directory.
fn temp_store() -> (SessionStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    // `new_at_base` is a pub(crate) constructor that creates the
    // `athena-sessions/` and `athena-images/` subdirs under `base`.
    let store = SessionStore::new_at_base(dir.path());
    (store, dir)
}

fn make_image_base64() -> String {
    let bytes: Vec<u8> = (0..64).map(|i| (i * 4) as u8).collect();
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}

fn make_session_message_with_image(image_ref: ImageRef) -> SessionMessage {
    SessionMessage {
        id: "msg-1".to_string(),
        role: MessageRole::User,
        content: "Here is an image".to_string(),
        timestamp: 1000,
        is_error: None,
        image_refs: Some(vec![image_ref]),
    }
}

#[tokio::test]
async fn test_create_session() {
    let (store, _dir) = temp_store();
    let session = store.create_session(Some("Test Session")).await.unwrap();
    assert_eq!(session.title, "Test Session");
    assert!(!session.id.is_empty());
    assert!(session.messages.is_empty());
}

#[tokio::test]
async fn test_get_session() {
    let (store, _dir) = temp_store();
    let session = store.create_session(Some("Get Test")).await.unwrap();
    let retrieved = store.get_session(&session.id).await.unwrap().unwrap();
    assert_eq!(retrieved.title, "Get Test");
    assert_eq!(retrieved.id, session.id);
}

#[tokio::test]
async fn test_rejects_path_traversal_session_id() {
    let (store, _dir) = temp_store();

    let result = store.get_session("../outside").await;

    assert!(matches!(result, Err(SessionStoreError::InvalidData(_))));
}

#[tokio::test]
async fn test_rejects_path_traversal_image_id() {
    let (store, _dir) = temp_store();

    let result = store.load_image("../outside").await;

    assert!(matches!(result, Err(SessionStoreError::InvalidData(_))));
}

#[tokio::test]
async fn test_update_session() {
    let (store, _dir) = temp_store();
    let session = store.create_session(Some("Original Title")).await.unwrap();
    let updated = store
        .update_session(&session.id, Some("Updated Title"), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.title, "Updated Title");
    let retrieved = store.get_session(&session.id).await.unwrap().unwrap();
    assert_eq!(retrieved.title, "Updated Title");
}

#[tokio::test]
async fn test_delete_session() {
    let (store, _dir) = temp_store();
    let session = store.create_session(Some("Delete Me")).await.unwrap();
    let deleted = store.delete_session(&session.id).await.unwrap();
    assert!(deleted);
    let retrieved = store.get_session(&session.id).await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_list_sessions() {
    let (store, _dir) = temp_store();
    store.create_session(Some("Session A")).await.unwrap();
    store.create_session(Some("Session B")).await.unwrap();
    store.create_session(Some("Session C")).await.unwrap();
    let list = store.list_sessions().await.unwrap();
    assert!(list.len() >= 3);
}

#[tokio::test]
async fn test_session_with_images() {
    let (store, _dir) = temp_store();
    let base64_data = make_image_base64();
    let image_ref = store
        .save_image(&base64_data, "image/png", Some("test.png".to_string()))
        .await
        .unwrap();
    let session = store
        .create_session(Some("Session with Image"))
        .await
        .unwrap();
    let msg = make_session_message_with_image(image_ref.clone());
    let updated = store
        .update_session(&session.id, None, Some(vec![msg]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.messages.len(), 1);
    let refs = &updated.messages[0].image_refs;
    assert!(refs.is_some());
    let refs = refs.as_ref().unwrap();
    assert_eq!(refs[0].image_id, image_ref.image_id);
    let loaded = store.load_image(&image_ref.image_id).await.unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap(), base64_data);
}

#[tokio::test]
async fn test_delete_session_keeps_images_referenced_by_another_session() {
    let (store, _dir) = temp_store();
    let image_ref = store
        .save_image(
            &make_image_base64(),
            "image/png",
            Some("shared.png".to_string()),
        )
        .await
        .unwrap();
    let first = store.create_session(Some("First")).await.unwrap();
    let second = store.create_session(Some("Second")).await.unwrap();
    let message = make_session_message_with_image(image_ref.clone());

    store
        .update_session(&first.id, None, Some(vec![message.clone()]))
        .await
        .unwrap();
    store
        .update_session(&second.id, None, Some(vec![message]))
        .await
        .unwrap();

    store.delete_session(&first.id).await.unwrap();

    assert!(store
        .load_image(&image_ref.image_id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn test_concurrent_updates_preserve_non_overlapping_changes() {
    let (store, _dir) = temp_store();
    let session = store.create_session(Some("Original")).await.unwrap();
    let store = std::sync::Arc::new(store);
    let title_store = std::sync::Arc::clone(&store);
    let message_store = std::sync::Arc::clone(&store);
    let session_id = session.id.clone();
    let message_session_id = session.id.clone();

    let (title_update, message_update) = tokio::join!(
        async move {
            title_store
                .update_session(&session_id, Some("Updated"), None)
                .await
        },
        async move {
            message_store
                .update_session(
                    &message_session_id,
                    None,
                    Some(vec![SessionMessage {
                        id: "message-1".to_string(),
                        role: MessageRole::User,
                        content: "content".to_string(),
                        timestamp: 1,
                        is_error: None,
                        image_refs: None,
                    }]),
                )
                .await
        }
    );
    title_update.unwrap();
    message_update.unwrap();

    let updated = store.get_session(&session.id).await.unwrap().unwrap();
    assert_eq!(updated.title, "Updated");
    assert_eq!(updated.messages.len(), 1);
}

#[tokio::test]
async fn test_cleanup_orphaned_images() {
    let (store, _dir) = temp_store();
    let base64_data = make_image_base64();
    let image_ref = store
        .save_image(&base64_data, "image/png", Some("orphan.png".to_string()))
        .await
        .unwrap();
    let loaded_before = store.load_image(&image_ref.image_id).await.unwrap();
    assert!(loaded_before.is_some());
    let removed = store.cleanup_orphaned_images().await.unwrap();
    assert!(removed >= 1);
    let loaded_after = store.load_image(&image_ref.image_id).await.unwrap();
    assert!(loaded_after.is_none());
}

#[tokio::test]
async fn test_empty_list() {
    let (store, _dir) = temp_store();
    let list = store.list_sessions().await.unwrap();
    // Fresh temp dir → must be empty.
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_session_id_uniqueness() {
    let (store, _dir) = temp_store();
    let s1 = store.create_session(Some("Unique 1")).await.unwrap();
    let s2 = store.create_session(Some("Unique 2")).await.unwrap();
    assert_ne!(s1.id, s2.id);
}
