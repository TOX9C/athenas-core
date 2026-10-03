use crate::types::{ChatSession, ImageRef, SessionListItem, SessionMessage};
use base64::Engine as _;
use serde_json;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

/// Compute a lowercase hex SHA-256 content hash for an image's raw bytes.
/// Used as a content-addressable identifier for dedup.
fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn validate_path_component(value: &str, kind: &str) -> Result<(), SessionStoreError> {
    let path = Path::new(value);
    let is_single_normal_component = path.components().count() == 1
        && matches!(
            path.components().next(),
            Some(std::path::Component::Normal(_))
        );
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.as_bytes().contains(&0)
        || value.contains('/')
        || value.contains('\\')
        || !is_single_normal_component
    {
        return Err(SessionStoreError::InvalidData(format!("invalid {kind}")));
    }
    Ok(())
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
    /// Raised when a session file fails to parse or validate. The offending
    /// file has been renamed to the path in the first field (a
    /// `<name>.corrupt-<millis>` sibling) so the corrupt bytes are preserved
    /// for inspection instead of being silently overwritten by a later save.
    #[error("Session file is corrupt and was quarantined to {0}: {1}")]
    Corrupt(PathBuf, String),
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Rename a corrupt session file to `<name>.corrupt-<millis>` so it is
/// preserved for inspection and no longer parsed or listed as a session.
/// Returns the quarantined path (or the original path if the rename failed).
fn quarantine_file(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("session.json"));
    let quarantined = path.with_file_name(format!("{name}.corrupt-{}", now_millis()));
    match std::fs::rename(path, &quarantined) {
        Ok(()) => {
            eprintln!(
                "SessionStore: CORRUPT session file {} quarantined to {}",
                path.display(),
                quarantined.display()
            );
            quarantined
        }
        Err(_) => path.to_path_buf(),
    }
}

// ---------------------------------------------------------------------------
// On-disk layout (F9)
// ---------------------------------------------------------------------------
//
// A session is split across two files in `sessions_dir`:
//   <id>.json  — small meta record (id/title/timestamps/count/preview),
//                rewritten cheaply on every mutation.
//   <id>.jsonl — append-only message log, one `SessionMessage` JSON object
//                per line. Appending a message costs O(one line) instead of
//                a full read-parse-serialize-rewrite of the whole session.
//
// Sessions written by the previous single-file layout (a full `ChatSession`
// JSON in <id>.json) are migrated lazily on first read: the messages are
// split into the log and the meta record replaces the file.

/// Meta record persisted at `sessions_dir/<id>.json`.
///
/// `deny_unknown_fields` distinguishes it from the legacy full-session file
/// (which carries a `messages` array) without a second parse pass.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionMeta {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "title")]
    title: String,
    #[serde(rename = "createdAt")]
    created_at: u64,
    #[serde(rename = "updatedAt")]
    updated_at: u64,
    #[serde(default, rename = "messageCount")]
    message_count: usize,
    #[serde(default, rename = "lastMessagePreview")]
    last_message_preview: String,
}

fn meta_from_session(session: &ChatSession) -> SessionMeta {
    SessionMeta {
        id: session.id.clone(),
        title: session.title.clone(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: session.messages.len(),
        last_message_preview: session
            .messages
            .last()
            .map(|m| m.content.chars().take(100).collect())
            .unwrap_or_default(),
    }
}

fn meta_path(sessions_dir: &Path, id: &str) -> Result<PathBuf, SessionStoreError> {
    validate_path_component(id, "session ID")?;
    Ok(sessions_dir.join(format!("{id}.json")))
}

fn messages_log_path(sessions_dir: &Path, id: &str) -> Result<PathBuf, SessionStoreError> {
    validate_path_component(id, "session ID")?;
    Ok(sessions_dir.join(format!("{id}.jsonl")))
}

fn write_meta(sessions_dir: &Path, meta: &SessionMeta) -> Result<(), SessionStoreError> {
    let path = meta_path(sessions_dir, &meta.id)?;
    write_file_durable(&path, serde_json::to_string(meta)?.as_bytes(), true)?;
    Ok(())
}

/// Replace the whole message log. Used for bulk updates and for eviction
/// compaction once a session runs at the `MAX_SESSION_MESSAGES` cap.
fn write_messages_log(
    sessions_dir: &Path,
    id: &str,
    messages: &[SessionMessage],
) -> Result<(), SessionStoreError> {
    let path = messages_log_path(sessions_dir, id)?;
    let mut buf = String::new();
    for msg in messages {
        let mut line = serde_json::to_string(msg)?;
        line.push('\n');
        buf.push_str(&line);
    }
    write_file_durable(&path, buf.as_bytes(), true)?;
    Ok(())
}

/// Append a single message to the log: one write + one fsync, O(line).
/// Callers hold the update lock, so appends cannot interleave.
fn append_message_to_log(
    sessions_dir: &Path,
    id: &str,
    msg: &SessionMessage,
) -> Result<(), SessionStoreError> {
    let path = messages_log_path(sessions_dir, id)?;
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

/// Read the message log. A missing log is an empty history. A torn final
/// line (crash mid-append) is dropped with a warning instead of
/// quarantining the whole log; interior corruption quarantines the file.
fn read_messages_log(
    sessions_dir: &Path,
    id: &str,
) -> Result<Vec<SessionMessage>, SessionStoreError> {
    let path = messages_log_path(sessions_dir, id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path)?;
    let mut out = Vec::new();
    let mut lines = data.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<SessionMessage>(line) {
            Ok(msg) => out.push(msg),
            Err(err) if lines.peek().is_none() => {
                // Torn tail from an interrupted append — drop just this line.
                eprintln!(
                    "SessionStore: dropping torn final message line in {} ({err})",
                    path.display()
                );
            }
            Err(err) => {
                let quarantined = quarantine_file(&path);
                return Err(SessionStoreError::Corrupt(quarantined, err.to_string()));
            }
        }
    }
    Ok(out)
}

/// Load a session synchronously: meta record + message log, migrating a
/// legacy single-file session on first read. `Ok(None)` when no meta file
/// exists for `id`.
fn load_session_sync(
    sessions_dir: &Path,
    id: &str,
) -> Result<Option<ChatSession>, SessionStoreError> {
    let path = meta_path(sessions_dir, id)?;
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)?;
    match serde_json::from_str::<SessionMeta>(&data) {
        Ok(meta) if !meta.id.is_empty() && !meta.title.is_empty() => {
            let messages = read_messages_log(sessions_dir, id)?;
            Ok(Some(ChatSession {
                id: meta.id,
                title: meta.title,
                created_at: meta.created_at,
                updated_at: meta.updated_at,
                messages,
            }))
        }
        _ => {
            // Legacy single-file layout (full ChatSession JSON) — migrate
            // once: split the messages into the log, replace with meta.
            match serde_json::from_str::<ChatSession>(&data) {
                Ok(session) if !session.id.is_empty() && !session.title.is_empty() => {
                    write_messages_log(sessions_dir, id, &session.messages)?;
                    write_meta(sessions_dir, &meta_from_session(&session))?;
                    Ok(Some(session))
                }
                Ok(_) => {
                    let quarantined = quarantine_file(&path);
                    Err(SessionStoreError::Corrupt(
                        quarantined,
                        "session file is missing a non-empty id or title".to_string(),
                    ))
                }
                Err(err) => {
                    let quarantined = quarantine_file(&path);
                    Err(SessionStoreError::Corrupt(quarantined, err.to_string()))
                }
            }
        }
    }
}

/// Manages session persistence and image storage.
/// Uses `~/.config/athena-core/athena-sessions/` for sessions,
/// and `~/.config/athena-core/athena-images/` for image data.
pub struct SessionStore {
    sessions_dir: PathBuf,
    images_dir: PathBuf,
    update_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
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
        crate::sweep_orphaned_temp_files(&sessions_dir, None);
        crate::sweep_orphaned_temp_files(&images_dir, None);
        SessionStore {
            sessions_dir,
            images_dir,
            update_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
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
        crate::sweep_orphaned_temp_files(&sessions_dir, None);
        crate::sweep_orphaned_temp_files(&images_dir, None);
        Ok(SessionStore {
            sessions_dir,
            images_dir,
            update_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
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
            crate::sweep_orphaned_temp_files(&sessions_dir_clone, None);
            crate::sweep_orphaned_temp_files(&images_dir_clone, None);
            Ok::<_, SessionStoreError>(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(SessionStore {
            sessions_dir,
            images_dir,
            update_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
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
            update_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn session_path(&self, id: &str) -> Result<PathBuf, SessionStoreError> {
        validate_path_component(id, "session ID")?;
        Ok(self.sessions_dir.join(format!("{id}.json")))
    }

    fn image_path(&self, image_id: &str) -> Result<PathBuf, SessionStoreError> {
        validate_path_component(image_id, "image ID")?;
        Ok(self.images_dir.join(format!("{image_id}.bin")))
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
        let path = self.image_path(&image_id)?;
        let path_clone = path.clone();
        // Only write if the file doesn't already exist — dedup at the FS level.
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            if !path_clone.exists() {
                write_file_durable(&path_clone, &buffer, false)?;
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
        let path = self.image_path(image_id)?;
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
        let path = self.image_path(image_id)?;
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
        let now = now_millis();
        let session = ChatSession {
            id: Uuid::new_v4().to_string(),
            title: title.unwrap_or("New Chat").to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };
        let sessions_dir = self.sessions_dir.clone();
        let meta = meta_from_session(&session);
        let id = session.id.clone();
        tokio::task::spawn_blocking(move || {
            write_meta(&sessions_dir, &meta)?;
            write_messages_log(&sessions_dir, &id, &[])?;
            Ok::<_, SessionStoreError>(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(session)
    }

    /// Retrieve a session by its ID.
    ///
    /// A file that fails to parse (or parses with an empty id/title, i.e.
    /// schema-invalid) is quarantined to `<id>.json.corrupt-<millis>` and a
    /// `SessionStoreError::Corrupt` is returned, so corrupt bytes are never
    /// silently overwritten by a later save while the contents stay available
    /// for inspection. Legacy single-file sessions are migrated on read.
    pub async fn get_session(&self, id: &str) -> Result<Option<ChatSession>, SessionStoreError> {
        self.session_path(id)?; // validates the path component
        let sessions_dir = self.sessions_dir.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || load_session_sync(&sessions_dir, &id))
            .await
            .map_err(|e| SessionStoreError::InvalidData(e.to_string()))?
    }

    /// Update a session's title and/or messages list.
    ///
    /// A title-only update rewrites just the small meta record; a messages
    /// update rewrites the message log once (a bulk replace, so the full
    /// rewrite is the requested operation, not per-message amplification).
    pub async fn update_session(
        &self,
        id: &str,
        title: Option<&str>,
        messages: Option<Vec<SessionMessage>>,
    ) -> Result<Option<ChatSession>, SessionStoreError> {
        let _update_guard = self.update_lock.lock().await;
        let mut session = self
            .get_session(id)
            .await?
            .ok_or(SessionStoreError::NotFound(id.to_string()))?;
        let rewrite_messages = messages.is_some();
        if let Some(t) = title {
            session.title = t.to_string();
        }
        if let Some(m) = messages {
            session.messages = m;
        }
        // Enforce the documented retention cap — a caller-supplied or
        // on-disk list larger than MAX_SESSION_MESSAGES is brought back
        // under the limit before it is written back.
        session.truncate_messages();
        session.updated_at = now_millis();

        let sessions_dir = self.sessions_dir.clone();
        let session_id = id.to_string();
        let messages_snapshot = session.messages.clone();
        let meta = meta_from_session(&session);
        tokio::task::spawn_blocking(move || -> Result<(), SessionStoreError> {
            if rewrite_messages {
                write_messages_log(&sessions_dir, &session_id, &messages_snapshot)?;
            }
            write_meta(&sessions_dir, &meta)?;
            Ok(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(Some(session))
    }

    /// Atomically append a message to a session under the update lock.
    /// Serialization with `update_session`/`delete_session` prevents the
    /// read-modify-write race where two concurrent writers each read the same
    /// base and lose each other's message.
    ///
    /// The append writes exactly one line to the append-only message log
    /// (`<id>.jsonl`) and rewrites only the small meta record — no full
    /// session-file rewrite per message. When `add_message_evicting` evicts
    /// (the session is at `MAX_SESSION_MESSAGES`), the log is compacted once.
    ///
    /// Returns the updated session, or `Ok(None)` when the session does not
    /// exist (callers distinguish "missing" from "updated" without a second
    /// lookup).
    pub async fn append_message(
        &self,
        id: &str,
        msg: SessionMessage,
    ) -> Result<Option<ChatSession>, SessionStoreError> {
        let _update_guard = self.update_lock.lock().await;
        let mut session = match self.get_session(id).await? {
            Some(s) => s,
            None => return Ok(None),
        };
        let len_before = session.messages.len();
        session.add_message_evicting(msg);
        let evicted = session.messages.len() < len_before + 1;
        session.updated_at = now_millis();

        let sessions_dir = self.sessions_dir.clone();
        let session_id = id.to_string();
        let append_line = session.messages.last().cloned();
        let messages_snapshot = session.messages.clone();
        let meta = meta_from_session(&session);
        tokio::task::spawn_blocking(move || -> Result<(), SessionStoreError> {
            if evicted {
                // At the cap every append also evicts the oldest line;
                // compact the log instead of leaving stale lines behind.
                write_messages_log(&sessions_dir, &session_id, &messages_snapshot)?;
            } else {
                append_message_to_log(
                    &sessions_dir,
                    &session_id,
                    append_line
                        .as_ref()
                        .expect("add_message_evicting always keeps the newest message"),
                )?;
            }
            write_meta(&sessions_dir, &meta)?;
            Ok(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        Ok(Some(session))
    }

    /// Delete a session and its associated images.
    ///
    /// Holds the update lock so a concurrent `update_session` cannot read
    /// the (about-to-be-deleted) file and resurrect the session by writing
    /// it back after the remove.
    pub async fn delete_session(&self, id: &str) -> Result<bool, SessionStoreError> {
        let guard = self.update_lock.lock().await;
        let meta = self.session_path(id)?;
        let log = {
            // Same directory + validated id, so reuse the validated path.
            let mut log_path = meta.clone();
            log_path.set_extension("jsonl");
            log_path
        };
        if !meta.exists() {
            return Ok(false);
        }
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            std::fs::remove_file(&meta)?;
            if log.exists() {
                std::fs::remove_file(&log)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        // Release before cleanup: cleanup_orphaned_images takes the same lock.
        drop(guard);
        let _ = self.cleanup_orphaned_images().await;
        Ok(true)
    }

    /// List all sessions with summary information.
    ///
    /// Summaries are built from the small per-session meta records only —
    /// message bodies are never parsed for a listing. A legacy single-file
    /// session is migrated (split into meta + log) the first time it is
    /// listed; corrupt files are quarantined and skipped, consistent with
    /// `get_session`.
    pub async fn list_sessions(&self) -> Result<Vec<SessionListItem>, SessionStoreError> {
        let sessions_dir = self.sessions_dir.clone();
        let mut sessions = tokio::task::spawn_blocking(
            move || -> Result<Vec<SessionListItem>, SessionStoreError> {
                let mut out = Vec::new();
                for entry in std::fs::read_dir(&sessions_dir)?.flatten() {
                    let path = entry.path();
                    if path.extension().is_none_or(|ext| ext != "json") {
                        continue;
                    }
                    let data = match std::fs::read_to_string(&path) {
                        Ok(data) => data,
                        Err(_) => continue,
                    };
                    let item = match serde_json::from_str::<SessionMeta>(&data) {
                        Ok(meta) if !meta.id.is_empty() && !meta.title.is_empty() => {
                            SessionListItem {
                                id: meta.id,
                                title: meta.title,
                                created_at: meta.created_at,
                                updated_at: meta.updated_at,
                                message_count: meta.message_count,
                                last_message_preview: meta.last_message_preview,
                            }
                        }
                        _ => {
                            // Legacy single-file session: migrate once so
                            // this (and every future) listing stays cheap.
                            match serde_json::from_str::<ChatSession>(&data) {
                                Ok(session)
                                    if !session.id.is_empty()
                                        && !session.title.is_empty() =>
                                {
                                    let item = SessionListItem {
                                        id: session.id.clone(),
                                        title: session.title.clone(),
                                        created_at: session.created_at,
                                        updated_at: session.updated_at,
                                        message_count: session.messages.len(),
                                        last_message_preview: session
                                            .messages
                                            .last()
                                            .map(|m| m.content.chars().take(100).collect())
                                            .unwrap_or_default(),
                                    };
                                    let id = session.id.clone();
                                    write_messages_log(
                                        &sessions_dir,
                                        &id,
                                        &session.messages,
                                    )?;
                                    write_meta(
                                        &sessions_dir,
                                        &meta_from_session(&session),
                                    )?;
                                    item
                                }
                                _ => {
                                    // Corrupt: quarantine so it is neither
                                    // listed nor re-parsed on every listing.
                                    let _ = quarantine_file(&path);
                                    continue;
                                }
                            }
                        }
                    };
                    out.push(item);
                }
                Ok(out)
            },
        )
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))??;
        // Sort by updated_at descending
        sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(sessions)
    }

    /// Clean up image files not referenced by any session message.
    ///
    /// Holds the update lock for the whole scan-and-delete so a concurrent
    /// `update_session`/`append_message` that references a freshly saved
    /// image cannot commit after the session scan and have the image swept
    /// out from under it. All filesystem work happens in one blocking task.
    pub async fn cleanup_orphaned_images(&self) -> Result<usize, SessionStoreError> {
        self.cleanup_orphaned_images_older_than(std::time::Duration::from_secs(60))
            .await
    }

    /// Same as [`Self::cleanup_orphaned_images`] but with an explicit
    /// "recently written" grace window (0s in tests).
    pub(crate) async fn cleanup_orphaned_images_older_than(
        &self,
        grace: std::time::Duration,
    ) -> Result<usize, SessionStoreError> {
        let _update_guard = self.update_lock.lock().await;
        let sessions_dir = self.sessions_dir.clone();
        let images_dir = self.images_dir.clone();
        tokio::task::spawn_blocking(move || -> Result<usize, SessionStoreError> {
            let mut used_image_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            // Collect referenced image ids from the append-only message logs
            // (`<id>.jsonl`) plus any not-yet-migrated legacy single-file
            // sessions. Meta records alone never reference images, so
            // nothing else needs parsing.
            for entry in std::fs::read_dir(&sessions_dir)?.flatten() {
                let path = entry.path();
                let is_jsonl = path.extension().is_some_and(|ext| ext == "jsonl");
                let is_json = path.extension().is_some_and(|ext| ext == "json");
                if !is_jsonl && !is_json {
                    continue;
                }
                let data = match std::fs::read_to_string(&path) {
                    Ok(data) => data,
                    Err(_) => continue,
                };
                let messages: Vec<SessionMessage> = if is_jsonl {
                    data.lines()
                        .filter(|l| !l.trim().is_empty())
                        .filter_map(|l| serde_json::from_str::<SessionMessage>(l).ok())
                        .collect()
                } else {
                    // Legacy layout: pull refs straight out of the embedded
                    // message array without materializing the session.
                    match serde_json::from_str::<ChatSession>(&data) {
                        Ok(session) => session.messages,
                        Err(_) => continue,
                    }
                };
                for msg in messages {
                    if let Some(refs) = &msg.image_refs {
                        for r in refs {
                            used_image_ids.insert(r.image_id.clone());
                        }
                    }
                }
            }

            let mut removed = 0;
            // Grace period: skip images still being written by an in-flight
            // save_image (its session reference may not be persisted yet —
            // save+update are separate calls, so an age-based floor closes
            // the sweep window without taking the lock across both).
            let recently_modified_cutoff = std::time::SystemTime::now().checked_sub(grace);
            for entry in std::fs::read_dir(&images_dir)?.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_stem() {
                    let image_id = name.to_string_lossy().to_string();
                    if used_image_ids.contains(&image_id) {
                        continue;
                    }
                    let freshly_written = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .is_some_and(|mtime| {
                            recently_modified_cutoff.is_some_and(|cutoff| mtime > cutoff)
                        });
                    if freshly_written {
                        continue;
                    }
                    if std::fs::remove_file(&path).is_ok() {
                        removed += 1;
                    }
                }
            }
            Ok(removed)
        })
        .await
        .map_err(|e| SessionStoreError::InvalidData(e.to_string()))?
    }
}

/// Persist session/image bytes through a unique same-directory temporary file.
/// The file is synced before rename and the parent directory is synced on Unix,
/// so a successful write is durable on the supported macOS release target.
/// For content-addressed images, an existing destination is accepted so racing
/// deduplicating writers do not fail.
fn write_file_durable(
    path: &std::path::Path,
    content: &[u8],
    replace_existing: bool,
) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("session.json"));
    let temp_path = parent.join(format!(".{name}.tmp-{}", Uuid::new_v4()));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        match std::fs::rename(&temp_path, path) {
            Ok(()) => Ok(()),
            Err(error)
                if !replace_existing && error.kind() == std::io::ErrorKind::AlreadyExists =>
            {
                let _ = std::fs::remove_file(&temp_path);
                Ok(())
            }
            Err(error) => Err(error),
        }
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result?;

    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            update_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        };
        (store, dir)
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn count_files_in_store(store: &SessionStore) -> usize {
        let dir = store
            .image_path("__probe__")
            .unwrap()
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

    /// Shared helper: a minimal user message.
    fn plain_message(id: &str) -> SessionMessage {
        SessionMessage {
            id: id.to_string(),
            role: crate::types::MessageRole::User,
            content: format!("content-{id}"),
            timestamp: 0,
            is_error: None,
            image_refs: None,
        }
    }

    /// Appends go to an append-only `<id>.jsonl` log: the log grows by one
    /// line per append instead of being rewritten whole.
    #[tokio::test]
    async fn append_message_writes_one_log_line() {
        let (store, _dir) = temp_store();
        let session = store.create_session(Some("Append")).await.unwrap();
        let log = store
            .sessions_dir
            .join(format!("{}.jsonl", session.id));
        assert!(log.exists(), "create_session writes an empty log");
        assert_eq!(std::fs::read_to_string(&log).unwrap().len(), 0);

        store
            .append_message(&session.id, plain_message("a-1"))
            .await
            .unwrap()
            .unwrap();
        let after_first = std::fs::read_to_string(&log).unwrap();
        assert_eq!(after_first.lines().count(), 1);

        store
            .append_message(&session.id, plain_message("a-2"))
            .await
            .unwrap()
            .unwrap();
        let after_second = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            after_second.lines().count(),
            2,
            "second append adds exactly one line"
        );
        assert!(
            after_second.starts_with(&after_first),
            "append must not rewrite the earlier lines"
        );

        let reloaded = store.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(reloaded.messages.len(), 2);
        assert_eq!(reloaded.messages[0].id, "a-1");
        assert_eq!(reloaded.messages[1].id, "a-2");
    }

    /// Legacy single-file sessions are migrated on first read.
    #[tokio::test]
    async fn legacy_single_file_session_migrates_on_read() {
        let (store, _dir) = temp_store();
        let legacy = ChatSession {
            id: "legacy-1".to_string(),
            title: "Legacy".to_string(),
            created_at: 1,
            updated_at: 2,
            messages: vec![plain_message("m-1"), plain_message("m-2")],
        };
        let meta_path = store.sessions_dir.join("legacy-1.json");
        std::fs::write(
            &meta_path,
            serde_json::to_string(&legacy).unwrap(),
        )
        .unwrap();

        // Listing migrates: subsequent listings read only the meta record.
        let listed = store.list_sessions().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "legacy-1");
        assert_eq!(listed[0].message_count, 2);
        assert_eq!(listed[0].last_message_preview, "content-m-2");
        assert!(store.sessions_dir.join("legacy-1.jsonl").exists());

        let loaded = store.get_session("legacy-1").await.unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].id, "m-1");

        // The meta file no longer embeds messages.
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert!(meta.get("messages").is_none());
        assert_eq!(meta["messageCount"], 2);
    }
}
