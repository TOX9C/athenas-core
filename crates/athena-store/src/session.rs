use crate::types::{ChatSession, ImageRef, SessionListItem, SessionMessage};
use base64::Engine as _;
use std::fs;
use std::path::PathBuf;
use serde_json;
use thiserror::Error;
use uuid::Uuid;

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
    /// Create the directories if they don't exist and return the store.
    pub fn new() -> Result<Self, SessionStoreError> {
        let base = dirs::data_dir().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()).join("athena-core");
        let sessions_dir = base.join("athena-sessions");
        let images_dir = base.join("athena-images");
        fs::create_dir_all(&sessions_dir)?;
        fs::create_dir_all(&images_dir)?;
        Ok(SessionStore { sessions_dir, images_dir })
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{id}.json"))
    }

    fn image_path(&self, image_id: &str) -> PathBuf {
        self.images_dir.join(format!("{image_id}.bin"))
    }

    /// Save an image (base64 string) to disk and return its reference.
    pub fn save_image(&self, base64_str: &str, media_type: &str, name: Option<String>) -> Result<ImageRef, SessionStoreError> {
        let image_id = Uuid::new_v4().to_string();
        let buffer = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))?;
        fs::write(self.image_path(&image_id), buffer)?;
        Ok(ImageRef { image_id, media_type: media_type.to_string(), name })
    }

    /// Load an image from disk and return its base64 encoding.
    pub fn load_image(&self, image_id: &str) -> Result<Option<String>, SessionStoreError> {
        let path = self.image_path(image_id);
        if !path.exists() {
            return Ok(None);
        }
        let buffer = fs::read(path)?;
        Ok(Some(base64::engine::general_purpose::STANDARD.encode(&buffer)))
    }

    /// Delete an image file from disk.  Swallows errors.
    pub fn delete_image(&self, image_id: &str) -> Result<(), SessionStoreError> {
        let path = self.image_path(image_id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Create a new session with the given title.
    pub fn create_session(&self, title: Option<&str>) -> Result<ChatSession, SessionStoreError> {
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
        fs::write(self.session_path(&session.id), json)?;
        Ok(session)
    }

    /// Retrieve a session by its ID.
    pub fn get_session(&self, id: &str) -> Result<Option<ChatSession>, SessionStoreError> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path)?;
        let session: ChatSession = serde_json::from_str(&data)
            .map_err(|e| SessionStoreError::Json(e))?;
        if session.id.is_empty() || session.title.is_empty() {
            return Ok(None);
        }
        Ok(Some(session))
    }

    /// Update a session's title and/or messages list.
    pub fn update_session(&self, id: &str, title: Option<&str>, messages: Option<Vec<SessionMessage>>) -> Result<Option<ChatSession>, SessionStoreError> {
        let mut session = self.get_session(id)?.ok_or(SessionStoreError::NotFound(id.to_string()))?;
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
        fs::write(self.session_path(id), json)?;
        Ok(Some(session))
    }

    /// Delete a session and its associated images.
    pub fn delete_session(&self, id: &str) -> Result<bool, SessionStoreError> {
        if let Some(session) = self.get_session(id)? {
            // Collect all image refs from messages
            let all_refs: Vec<&str> = session.messages.iter()
                .flat_map(|m| m.image_refs.as_ref().map(|refs| refs.iter().map(|r| r.image_id.as_str()))
                    .into_iter().flatten()
                )
                .collect();
            for image_id in all_refs {
                let _ = self.delete_image(image_id);
            }
            fs::remove_file(self.session_path(id))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all sessions with summary information.
    pub fn list_sessions(&self) -> Result<Vec<SessionListItem>, SessionStoreError> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext != "json" { continue; }
            } else {
                continue;
            }
            let data = match std::fs::read_to_string(&path) {
                Ok(d) => d,
                Err(_) => continue,
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
                last_message_preview: last_msg.map(|m| {
                    let content = &m.content;
                    if content.len() > 100 { &content[..100] } else { content }.to_string()
                }).unwrap_or_default(),
            });
        }
        // Sort by updated_at descending
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Clean up image files not referenced by any session message.
    pub fn cleanup_orphaned_images(&self) -> Result<usize, SessionStoreError> {
        let mut used_image_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for entry in std::fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            let data = match std::fs::read_to_string(&path) {
                Ok(d) => d,
                Err(_) => continue,
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
        for entry in std::fs::read_dir(&self.images_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_stem() {
                let image_id = name.to_string_lossy().to_string();
                if !used_image_ids.contains(&image_id) {
                    if let Err(_) = std::fs::remove_file(&path) {
                        // skip errors
                    } else {
                        removed += 1;
                    }
                }
            }
        }
        Ok(removed)
    }
}
