use crate::types::{ChatSession, ImageRef, SessionListItem, SessionMessage};
use base64::Engine as _;
use serde_json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Compute a lowercase hex SHA-256 content hash for an image's raw bytes.
/// Used as a content-addressable identifier for dedup.
fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[derive(Error, Debug)]
pub enum SessionStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid data or corrupt file: {0}")]
    InvalidData(String),
}

/// Manages session persistence and image storage.
/// Uses `~/.config/athena-core/athena-sessions/` for sessions,
/// and `~/.config/athena-core/athena-images/` for image data.
pub struct SessionStore {
    sessions_dir: PathBuf,
    images_dir: PathBuf,
}

impl SessionStore {
    /// Empty fallback constructor — creates a store pointing at a temp directory.
    /// Used when the real data directory is inaccessible at startup.
    pub fn new_empty() -> Self {
        let base = std::env::temp_dir().join("athena-core-fallback");
        let sessions_dir = base.join("athena-sessions");
        let images_dir = base.join("athena-images");
        let _ = std::fs::create_dir_all(&sessions_dir);
        let _ = std::fs::create_dir_all(&images_dir);
        SessionStore {
            sessions_dir,
            images_dir,
        }
    }

    /// Create the directories if they don't exist and return the store.
    /// Synchronous version for initialization (blocking is acceptable at startup).
    pub fn new_sync() -> Result<Self, SessionStoreError> {
        let base = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("athena-core");
        let sessions_dir = base.join("athena-sessions");
        let images_dir = base.join("athena-images");
        std::fs::create_dir_all(&sessions_dir)?;
        std::fs::create_dir_all(&images_dir)?;
        Ok(SessionStore {
            sessions_dir,
            images_dir,
        })
    }

    /// Async version for runtime creation.
    pub async fn new() -> Result<Self, SessionStoreError> {
        let base = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("athena-core");
        let sessions_dir = base.join("athena-sessions");
        let images_dir = base.join("athena-images");
        let sessions_dir_clone = sessions_dir.clone();
        let images_dir_clone = images_dir.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&sessions_dir_clone)?;
            std::fs::create_dir_all(&images_dir_clone)?;
            Ok::<_, SessionStoreError>(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(SessionStore {
            sessions_dir,
            images_dir,
        })
    }

    /// Construct a SessionStore rooted at a caller-provided base directory.
    /// Creates `athena-sessions/` and `athena-images/` subdirs.  Used by
    /// integration tests (`session_tests.rs`) to get isolated temp dirs
    /// without touching the user's real data directory.
    #[cfg(test)]
    pub(crate) fn new_at_base(base: &std::path::Path) -> Self {
        let sessions_dir = base.join("athena-sessions");
        let images_dir = base.join("athena-images");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::create_dir_all(&images_dir).unwrap();
        SessionStore {
            sessions_dir,
            images_dir,
        }
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{id}.json"))
    }

    fn image_path(&self, image_id: &str) -> PathBuf {
        self.images_dir.join(format!("{image_id}.bin"))
    }

    /// Save an image (base64 string) to disk and return its reference.
    ///
    /// Content-addressable: the image is stored at `<sha256>.bin` keyed by the
    /// SHA-256 of the raw bytes. Saving the same image twice returns the same
    /// `image_id` and writes to disk only once. The first base64 decode still
    /// happens, but later loads of the same image skip the on-disk write.
    pub async fn save_image(
        &self,
        base64_str: &str,
        media_type: &str,
        name: Option<String>,
    ) -> Result<ImageRef, SessionStoreError> {
        let buffer = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))?;
        let image_id = content_hash(&buffer);
        let path = self.image_path(&image_id);
        let path_clone = path.clone();
        // Only write if the file doesn't already exist — dedup at the FS level.
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            if !path_clone.exists() {
                std::fs::write(&path_clone, &buffer)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(ImageRef {
            image_id,
            media_type: media_type.to_string(),
            name,
        })
    }

    /// Load an image from disk and return its base64 encoding.
    ///
    /// The on-disk path is `<image_id>.bin`. New images use a 64-char SHA-256
    /// hash as their `image_id` (content-addressable). Sessions written before
    /// the hash change used UUIDs — those still resolve correctly because the
    /// `image_path` helper builds `<id>.bin` for any string.
    pub async fn load_image(&self, image_id: &str) -> Result<Option<String>, SessionStoreError> {
        let path = self.image_path(image_id);
        if !path.exists() {
            return Ok(None);
        }
        let path_clone = path.clone();
        let buffer = tokio::task::spawn_blocking(move || std::fs::read(&path_clone))
            .await
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(Some(
            base64::engine::general_purpose::STANDARD.encode(&buffer),
        ))
    }

    /// Delete an image file from disk. Swallows errors.
    pub async fn delete_image(&self, image_id: &str) -> Result<(), SessionStoreError> {
        let path = self.image_path(image_id);
        if path.exists() {
            let path_clone = path.clone();
            tokio::task::spawn_blocking(move || std::fs::remove_file(&path_clone))
                .await
                .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        }
        Ok(())
    }

    /// Create a new session with the given title.
    pub async fn create_session(
        &self,
        title: Option<&str>,
    ) -> Result<ChatSession, SessionStoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let session = ChatSession {
            id: Uuid::new_v4().to_string(),
            title: title.unwrap_or("New Chat").to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&session)?;
        let path = self.session_path(&session.id);
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || std::fs::write(&path_clone, json))
            .await
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(session)
    }

    /// Retrieve a session by its ID.
    pub async fn get_session(&self, id: &str) -> Result<Option<ChatSession>, SessionStoreError> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let path_clone = path.clone();
        let data = tokio::task::spawn_blocking(move || std::fs::read_to_string(&path_clone))
            .await
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        let session: ChatSession = serde_json::from_str(&data).map_err(SessionStoreError::Json)?;
        if session.id.is_empty() || session.title.is_empty() {
            return Ok(None);
        }
        Ok(Some(session))
    }

    /// Update a session's title and/or messages list.
    pub async fn update_session(
        &self,
        id: &str,
        title: Option<&str>,
        messages: Option<Vec<SessionMessage>>,
    ) -> Result<Option<ChatSession>, SessionStoreError> {
        let mut session = self
            .get_session(id)
            .await?
            .ok_or(SessionStoreError::NotFound(id.to_string()))?;
        if let Some(t) = title {
            session.title = t.to_string();
        }
        if let Some(m) = messages {
            session.messages = m;
        }
        session.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let json = serde_json::to_string_pretty(&session)?;
        let path = self.session_path(id);
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || std::fs::write(&path_clone, json))
            .await
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(Some(session))
    }

    /// Delete a session and its associated images.
    pub async fn delete_session(&self, id: &str) -> Result<bool, SessionStoreError> {
        if let Some(session) = self.get_session(id).await? {
            // Collect all image refs from messages
            let all_refs: Vec<&str> = session
                .messages
                .iter()
                .flat_map(|m| {
                    m.image_refs
                        .as_ref()
                        .map(|refs| refs.iter().map(|r| r.image_id.as_str()))
                        .into_iter()
                        .flatten()
                })
                .collect();
            for image_id in all_refs {
                let _ = self.delete_image(image_id).await;
            }
            let path = self.session_path(id);
            let path_clone = path.clone();
            tokio::task::spawn_blocking(move || std::fs::remove_file(&path_clone))
                .await
                .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all sessions with summary information.
    pub async fn list_sessions(&self) -> Result<Vec<SessionListItem>, SessionStoreError> {
        let entries = tokio::task::spawn_blocking({
            let sessions_dir = self.sessions_dir.clone();
            move || -> Result<Vec<_>, SessionStoreError> {
                Ok(std::fs::read_dir(&sessions_dir)?.collect::<Vec<_>>())
            }
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;

        let mut sessions = Vec::new();
        for entry_result in entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext != "json" {
                    continue;
                }
            } else {
                continue;
            }
            let path_clone = path.clone();
            let data =
                match tokio::task::spawn_blocking(move || std::fs::read_to_string(&path_clone))
                    .await
                {
                    Ok(Ok(d)) => d,
                    _ => continue,
                };
            let session: ChatSession = match serde_json::from_str(&data) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let last_msg = session.messages.last();
            sessions.push(SessionListItem {
                id: session.id,
                title: session.title,
                created_at: session.created_at,
                updated_at: session.updated_at,
                message_count: session.messages.len(),
                last_message_preview: last_msg
                    .map(|m| m.content.chars().take(100).collect())
                    .unwrap_or_default(),
            });
        }
        // Sort by updated_at descending
        sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(sessions)
    }

    /// Clean up image files not referenced by any session message.
    pub async fn cleanup_orphaned_images(&self) -> Result<usize, SessionStoreError> {
        let mut used_image_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        let session_entries = tokio::task::spawn_blocking({
            let sessions_dir = self.sessions_dir.clone();
            move || -> Result<Vec<_>, SessionStoreError> {
                Ok(std::fs::read_dir(&sessions_dir)?.collect::<Vec<_>>())
            }
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;

        for entry_result in session_entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let path_clone = path.clone();
            let data =
                match tokio::task::spawn_blocking(move || std::fs::read_to_string(&path_clone))
                    .await
                {
                    Ok(Ok(d)) => d,
                    _ => continue,
                };
            let session: ChatSession = match serde_json::from_str(&data) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for msg in &session.messages {
                if let Some(refs) = &msg.image_refs {
                    for r in refs {
                        used_image_ids.insert(r.image_id.clone());
                    }
                }
            }
        }

        let mut removed = 0;
        let image_entries = tokio::task::spawn_blocking({
            let images_dir = self.images_dir.clone();
            move || -> Result<Vec<_>, SessionStoreError> {
                Ok(std::fs::read_dir(&images_dir)?.collect::<Vec<_>>())
            }
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;

        for entry_result in image_entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if let Some(name) = path.file_stem() {
                let image_id = name.to_string_lossy().to_string();
                if !used_image_ids.contains(&image_id) {
                    let path_clone = path.clone();
                    if tokio::task::spawn_blocking(move || std::fs::remove_file(&path_clone))
                        .await
                        .is_ok()
                    {
                        removed += 1;
                    }
                }
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    /// Build a SessionStore rooted in a fresh temp directory. Each test gets
    /// an isolated images directory, so file counts are deterministic.
    fn temp_store() -> (SessionStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let sessions_dir = dir.path().join("sessions");
        let images_dir = dir.path().join("images");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::create_dir_all(&images_dir).unwrap();
        // Child module can construct via private fields, avoiding the
        // shared-state of new_empty()/new_sync().
        let store = SessionStore {
            sessions_dir,
            images_dir,
        };
        (store, dir)
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn count_files_in_store(store: &SessionStore) -> usize {
        let dir = store
            .image_path("__probe__")
            .parent()
            .unwrap()
            .to_path_buf();
        std::fs::read_dir(&dir)
            .map(|it| it.flatten().count())
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn save_image_dedupes_identical_content() {
        let (store, _dir) = temp_store();
        let bytes = b"fake-png-bytes-for-dedup-test";
        let input = b64(bytes);

        let ref1 = store
            .save_image(&input, "image/png", Some("a".into()))
            .await
            .expect("save 1");
        assert_eq!(count_files_in_store(&store), 1, "first save creates file");

        let ref2 = store
            .save_image(&input, "image/png", Some("a".into()))
            .await
            .expect("save 2");
        assert_eq!(
            count_files_in_store(&store),
            1,
            "second save must NOT create a new file"
        );
        assert_eq!(
            ref1.image_id, ref2.image_id,
            "identical content yields identical id"
        );
    }

    #[tokio::test]
    async fn load_image_returns_correct_data() {
        let (store, _dir) = temp_store();
        let bytes = b"hello-image-bytes";
        let input = b64(bytes);

        let img_ref = store
            .save_image(&input, "image/png", Some("hi".into()))
            .await
            .expect("save");
        let loaded = store
            .load_image(&img_ref.image_id)
            .await
            .expect("load")
            .expect("present");
        assert_eq!(loaded, input, "loaded base64 must round-trip");
    }

    #[tokio::test]
    async fn different_images_get_different_ids() {
        let (store, _dir) = temp_store();
        let a = b64(b"alpha-image");
        let b = b64(b"beta-image");

        let ref_a = store.save_image(&a, "image/png", None).await.unwrap();
        let ref_b = store.save_image(&b, "image/png", None).await.unwrap();
        assert_ne!(ref_a.image_id, ref_b.image_id);
        assert_eq!(count_files_in_store(&store), 2);
    }
}
