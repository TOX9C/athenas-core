use dioxus::prelude::*;

use super::athena::AthenaMessage;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Summary of a session for the session list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionListItem {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub last_message_preview: String,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Maximum number of messages retained for the active session.
const MAX_ACTIVE_MESSAGES: usize = 500;

/// Global session management state.
#[derive(Clone, PartialEq, Default)]
pub struct SessionState {
    pub sessions: Vec<SessionListItem>,
    pub active_session_id: Option<String>,
    pub active_session_messages: Vec<AthenaMessage>,
    pub is_sessions_loaded: bool,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_session_id: None,
            active_session_messages: Vec::new(),
            is_sessions_loaded: false,
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    /// Mark sessions as loaded.
    pub fn mark_sessions_loaded(&mut self) {
        self.is_sessions_loaded = true;
    }

    /// Set the full session list (e.g. after loading from backend).
    pub fn set_sessions(&mut self, sessions: Vec<SessionListItem>) {
        self.sessions = sessions;
        self.is_sessions_loaded = true;
    }

    /// Create a new session and make it active.
    pub fn create_session(&mut self, id: impl Into<String>, title: impl Into<String>, now: i64) {
        let session_id = id.into();
        let session_title = title.into();
        let item = SessionListItem {
            id: session_id.clone(),
            title: session_title,
            created_at: now,
            updated_at: now,
            message_count: 0,
            last_message_preview: String::new(),
        };
        self.sessions.insert(0, item);
        self.active_session_id = Some(session_id);
        self.active_session_messages.clear();
    }

    /// Switch to an existing session by id, replacing active messages.
    pub fn switch_session(&mut self, id: &str, messages: Vec<AthenaMessage>) {
        self.active_session_id = Some(id.to_string());
        self.active_session_messages = messages;
    }

    /// Delete a session by id. If it was active, switch to the first remaining
    /// session or clear the active state.
    pub fn delete_session(&mut self, id: &str) {
        self.sessions.retain(|s| s.id != id);
        if self.active_session_id.as_deref() == Some(id) {
            self.active_session_messages.clear();
            if let Some(first) = self.sessions.first() {
                self.active_session_id = Some(first.id.clone());
            } else {
                self.active_session_id = None;
            }
        }
    }

    /// Append a message to the active session and update the list item preview.
    pub fn add_message_to_active_session(&mut self, msg: AthenaMessage) {
        if self.active_session_id.is_none() {
            return;
        }
        let preview: String = msg.content.chars().take(100).collect();
        self.active_session_messages.push(msg);
        if self.active_session_messages.len() > MAX_ACTIVE_MESSAGES {
            let excess = self.active_session_messages.len() - MAX_ACTIVE_MESSAGES;
            self.active_session_messages.drain(0..excess);
        }

        if let Some(active_id) = &self.active_session_id {
            if let Some(item) = self.sessions.iter_mut().find(|s| &s.id == active_id) {
                item.message_count += 1;
                item.last_message_preview = preview;
            }
        }
    }

    /// Update a session's title.
    pub fn update_session_title(&mut self, id: &str, title: impl Into<String>) {
        if let Some(item) = self.sessions.iter_mut().find(|s| s.id == id) {
            item.title = title.into();
        }
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the session signal from the Dioxus context.
pub fn use_session_store() -> Signal<SessionState> {
    use_context::<Signal<SessionState>>()
}

/// Initialize the session store as a context provider.
pub fn provide_session_store() {
    use_context_provider(|| Signal::new(SessionState::new()));
}
