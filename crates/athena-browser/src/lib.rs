use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use thiserror::Error;

use athena_core::EventEmitter;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during browser manager operations.
#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("Panel not found: {0}")]
    PanelNotFound(String),
    #[error("Panel already exists: {0}")]
    PanelAlreadyExists(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Lock poisoned: {0}")]
    LockPoison(String),
    #[error("No back history for panel: {0}")]
    NoBackHistory(String),
    #[error("No forward history for panel: {0}")]
    NoForwardHistory(String),
}

// ---------------------------------------------------------------------------
// Loading state
// ---------------------------------------------------------------------------

/// Represents the loading state of a browser panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LoadingState {
    /// The panel is idle — no navigation in progress.
    #[default]
    Idle,
    /// The panel is currently loading a page.
    Loading,
    /// The last navigation failed.
    Failed,
}

// ---------------------------------------------------------------------------
// Navigation history
// ---------------------------------------------------------------------------

/// Tracks navigation history for a single browser panel, supporting
/// back/forward traversal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NavigationHistory {
    /// URLs visited before the current page (most-recent-first).
    back_stack: Vec<String>,
    /// URLs visited after the current page (most-recent-first).
    forward_stack: Vec<String>,
    /// Maximum number of entries retained in each stack.
    max_entries: usize,
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self::new(50)
    }
}

impl NavigationHistory {
    /// Create a new history with the given capacity per stack.
    pub fn new(max_entries: usize) -> Self {
        Self {
            back_stack: Vec::with_capacity(max_entries),
            forward_stack: Vec::with_capacity(max_entries),
            max_entries,
        }
    }

    /// Record a navigation to `url`. The caller is expected to have already
    /// updated the panel's current URL before calling this if the previous
    /// URL should be preserved in the back stack.
    fn push_back(&mut self, url: String) {
        self.back_stack.push(url);
        self.forward_stack.clear();
        self.trim_back();
    }

    /// Navigate back: pop the back stack, push the current URL onto the
    /// forward stack, and return the URL to navigate to.
    fn go_back(&mut self, current_url: String) -> Option<String> {
        let target = self.back_stack.pop()?;
        self.forward_stack.push(current_url);
        self.trim_forward();
        Some(target)
    }

    /// Navigate forward: pop the forward stack, push the current URL onto the
    /// back stack, and return the URL to navigate to.
    fn go_forward(&mut self, current_url: String) -> Option<String> {
        let target = self.forward_stack.pop()?;
        self.back_stack.push(current_url);
        self.trim_back();
        Some(target)
    }

    /// Whether a back navigation is possible.
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    /// Whether a forward navigation is possible.
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    /// Number of entries in the back stack.
    pub fn back_count(&self) -> usize {
        self.back_stack.len()
    }

    /// Number of entries in the forward stack.
    pub fn forward_count(&self) -> usize {
        self.forward_stack.len()
    }

    fn trim_back(&mut self) {
        if self.back_stack.len() > self.max_entries {
            let excess = self.back_stack.len() - self.max_entries;
            self.back_stack.drain(..excess);
        }
    }

    fn trim_forward(&mut self) {
        if self.forward_stack.len() > self.max_entries {
            let excess = self.forward_stack.len() - self.max_entries;
            self.forward_stack.drain(..excess);
        }
    }
}

// ---------------------------------------------------------------------------
// Browser panel
// ---------------------------------------------------------------------------

/// State for a single embedded browser panel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserPanel {
    /// Unique identifier for this panel.
    pub id: String,
    /// The currently displayed URL (empty if no page has been loaded).
    pub current_url: String,
    /// The page title, if known.
    pub title: String,
    /// Navigation history (back / forward stacks).
    pub history: NavigationHistory,
    /// Current loading state.
    pub loading_state: LoadingState,
}

impl BrowserPanel {
    fn new(id: String, url: String) -> Self {
        Self {
            id,
            current_url: url,
            title: String::new(),
            history: NavigationHistory::default(),
            loading_state: LoadingState::Loading,
        }
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Ensure a bare hostname gets the `https://` prefix, mirroring the Electron
/// `browserManager.ts` behaviour where URLs without a scheme are treated as
/// HTTPS.
pub fn normalize_url(raw: &str) -> Result<String, BrowserError> {
    let trimmed = raw.trim();
    // Reject dangerous URI schemes
    let lower = trimmed.to_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("file:")
    {
        return Err(BrowserError::InvalidUrl(format!(
            "Scheme not allowed: {}",
            trimmed
        )));
    }
    if trimmed.is_empty() {
        return Err(BrowserError::InvalidUrl("URL is empty".to_string()));
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed)
    };
    // Basic structural validation — a full URL parse would require the `url`
    // crate, but we keep deps minimal and validate what matters.
    let is_localhost = with_scheme.starts_with("http://localhost")
        && with_scheme["http://localhost".len()..]
            .chars()
            .next()
            .is_none_or(|c| c == ':' || c == '/' || c == '?' || c == '#')
        || with_scheme.starts_with("https://localhost")
            && with_scheme["https://localhost".len()..]
                .chars()
                .next()
                .is_none_or(|c| c == ':' || c == '/' || c == '?' || c == '#');
    if !with_scheme.contains('.') && !is_localhost {
        return Err(BrowserError::InvalidUrl(format!(
            "URL lacks a domain: {}",
            with_scheme
        )));
    }
    Ok(with_scheme)
}

// ---------------------------------------------------------------------------
// Browser manager
// ---------------------------------------------------------------------------

/// Thread-safe manager for embedded browser panels.
///
/// This is the pure data / model layer. Actual Tauri webview operations are
/// performed in the `src-tauri` command handlers that call into this struct.
pub struct BrowserManager {
    panels: Arc<RwLock<HashMap<String, BrowserPanel>>>,
    event_emitter: EventEmitter,
}

impl std::fmt::Debug for BrowserManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserManager")
            .field("panels", &"<RwLock<HashMap>>")
            .field("event_emitter", &"<Option>")
            .finish()
    }
}

impl Clone for BrowserManager {
    fn clone(&self) -> Self {
        Self {
            panels: Arc::clone(&self.panels),
            event_emitter: Arc::clone(&self.event_emitter),
        }
    }
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserManager {
    /// Create a new, empty `BrowserManager`.
    pub fn new() -> Self {
        Self {
            panels: Arc::new(RwLock::new(HashMap::new())),
            event_emitter: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        if let Ok(guard) = self.event_emitter.lock() {
            if let Some(ref emitter) = *guard {
                emitter(channel, data);
                return;
            }
        }
        log::debug!("[browser] {} -> {}", channel, data);
    }

    // -- Panel lifecycle ----------------------------------------------------

    /// Open a new browser panel identified by `id` and navigate it to `url`.
    ///
    /// Returns `Err(BrowserError::PanelAlreadyExists)` if a panel with the
    /// same ID is already open.
    pub fn open_browser(&self, id: impl Into<String>, url: &str) -> Result<(), BrowserError> {
        let id = id.into();
        let normalized = normalize_url(url)?;

        let mut panels = self.write_lock()?;

        if panels.contains_key(&id) {
            return Err(BrowserError::PanelAlreadyExists(id));
        }

        let panel = BrowserPanel::new(id.clone(), normalized);
        panels.insert(id, panel);
        Ok(())
    }

    /// Close (remove) a browser panel by ID.
    pub fn close_browser(&self, id: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        panels
            .remove(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(())
    }

    /// Check whether a panel with the given ID exists.
    pub fn has_panel(&self, id: &str) -> Result<bool, BrowserError> {
        Ok(self.read_lock()?.contains_key(id))
    }

    /// Return the IDs of all open panels.
    pub fn panel_ids(&self) -> Result<Vec<String>, BrowserError> {
        Ok(self.read_lock()?.keys().cloned().collect())
    }

    // -- Navigation ---------------------------------------------------------

    /// Navigate an existing panel to a new URL, recording the previous URL in
    /// the back history.
    pub fn navigate(&self, id: &str, url: &str) -> Result<(), BrowserError> {
        let normalized = normalize_url(url)?;

        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;

        if !panel.current_url.is_empty() {
            panel.history.push_back(panel.current_url.clone());
        }
        panel.current_url = normalized;
        panel.loading_state = LoadingState::Loading;
        panel.title.clear();
        Ok(())
    }

    /// Navigate back in the panel's history.
    ///
    /// Returns the URL to navigate to, or `Err(BrowserError::NoBackHistory)`.
    pub fn go_back(&self, id: &str) -> Result<String, BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;

        if !panel.history.can_go_back() {
            return Err(BrowserError::NoBackHistory(id.to_string()));
        }

        let previous = panel.current_url.clone();
        let target = panel
            .history
            .go_back(previous)
            .expect("can_go_back was true but go_back returned None");

        panel.current_url = target.clone();
        panel.loading_state = LoadingState::Loading;
        panel.title.clear();
        Ok(target)
    }

    /// Navigate forward in the panel's history.
    ///
    /// Returns the URL to navigate to, or `Err(BrowserError::NoForwardHistory)`.
    pub fn go_forward(&self, id: &str) -> Result<String, BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;

        if !panel.history.can_go_forward() {
            return Err(BrowserError::NoForwardHistory(id.to_string()));
        }

        let previous = panel.current_url.clone();
        let target = panel
            .history
            .go_forward(previous)
            .expect("can_go_forward was true but go_forward returned None");

        panel.current_url = target.clone();
        panel.loading_state = LoadingState::Loading;
        panel.title.clear();
        Ok(target)
    }

    /// Reload the current page. This resets the loading state without
    /// changing the URL or history.
    pub fn reload(&self, id: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;

        panel.loading_state = LoadingState::Loading;
        panel.title.clear();
        Ok(())
    }

    // -- Queries ------------------------------------------------------------

    /// Get the currently displayed URL for a panel.
    pub fn get_active_url(&self, id: &str) -> Result<String, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.current_url.clone())
    }

    /// Check whether a panel is currently loading a page.
    pub fn is_loading(&self, id: &str) -> Result<bool, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.loading_state == LoadingState::Loading)
    }

    /// Get the full loading state for a panel.
    pub fn loading_state(&self, id: &str) -> Result<LoadingState, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.loading_state)
    }

    /// Get the page title for a panel.
    pub fn get_title(&self, id: &str) -> Result<String, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.title.clone())
    }

    /// Whether the panel can navigate back.
    pub fn can_go_back(&self, id: &str) -> Result<bool, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.history.can_go_back())
    }

    /// Whether the panel can navigate forward.
    pub fn can_go_forward(&self, id: &str) -> Result<bool, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.history.can_go_forward())
    }

    /// Get a snapshot of the panel state (URL, title, loading, history).
    pub fn get_panel(&self, id: &str) -> Result<BrowserPanel, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.clone())
    }

    // -- State updates (called by Tauri command layer / event handlers) ------

    /// Mark that a page has finished loading successfully.
    pub fn set_loaded(&self, id: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        panel.loading_state = LoadingState::Idle;
        Ok(())
    }

    /// Mark that a page load has failed.
    pub fn set_load_failed(&self, id: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        panel.loading_state = LoadingState::Failed;
        Ok(())
    }

    /// Update the URL of a panel (e.g. on in-page navigation) without
    /// affecting the back/forward history.
    pub fn set_url(&self, id: &str, url: &str) -> Result<(), BrowserError> {
        let normalized = normalize_url(url)?;
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        panel.current_url = normalized.clone();
        let panel_id = id.to_string();
        drop(panels);

        self.emit_event(
            "browser:urlChange",
            &serde_json::json!({
                "id": panel_id,
                "url": normalized,
            }),
        );

        Ok(())
    }

    /// Update the page title for a panel.
    pub fn set_title(&self, id: &str, title: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        panel.title = title.to_string();
        let panel_id = id.to_string();
        let title_str = title.to_string();
        drop(panels);

        self.emit_event(
            "browser:titleChange",
            &serde_json::json!({
                "id": panel_id,
                "title": title_str,
            }),
        );

        Ok(())
    }

    // -- Shutdown -----------------------------------------------------------

    /// Close all panels.
    pub fn shutdown(&self) {
        if let Ok(mut panels) = self.write_lock() {
            panels.clear();
        }
    }

    // -- Lock helpers -------------------------------------------------------

    fn read_lock(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, BrowserPanel>>, BrowserError> {
        self.panels
            .read()
            .map_err(|_| BrowserError::LockPoison("browser panels".to_string()))
    }

    fn write_lock(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<String, BrowserPanel>>, BrowserError> {
        self.panels
            .write()
            .map_err(|_| BrowserError::LockPoison("browser panels".to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn new_manager() -> BrowserManager {
        BrowserManager::new()
    }

    // -- normalize_url ------------------------------------------------------

    #[test]
    fn normalize_url_adds_https_prefix() {
        assert_eq!(normalize_url("example.com").unwrap(), "https://example.com");
    }

    #[test]
    fn normalize_url_preserves_http() {
        assert_eq!(
            normalize_url("http://example.com").unwrap(),
            "http://example.com"
        );
    }

    #[test]
    fn normalize_url_preserves_https() {
        assert_eq!(
            normalize_url("https://example.com").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_url_trims_whitespace() {
        assert_eq!(
            normalize_url("  example.com  ").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_url_rejects_empty() {
        assert!(normalize_url("").is_err());
        assert!(normalize_url("   ").is_err());
    }

    #[test]
    fn normalize_url_rejects_no_domain() {
        assert!(normalize_url("localhost-no-dot").is_err());
    }

    #[test]
    fn normalize_url_allows_localhost() {
        assert_eq!(
            normalize_url("http://localhost:3000").unwrap(),
            "http://localhost:3000"
        );
    }

    // -- Panel lifecycle ----------------------------------------------------

    #[test]
    fn open_browser_creates_panel() {
        let mgr = new_manager();
        assert!(mgr.open_browser("p1", "https://google.com").is_ok());
        assert!(mgr.has_panel("p1").unwrap());
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://google.com");
    }

    #[test]
    fn open_browser_normalizes_url() {
        let mgr = new_manager();
        assert!(mgr.open_browser("p1", "google.com").is_ok());
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://google.com");
    }

    #[test]
    fn open_browser_rejects_duplicate_id() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        let err = mgr.open_browser("p1", "https://b.com").unwrap_err();
        assert!(matches!(err, BrowserError::PanelAlreadyExists(_)));
    }

    #[test]
    fn close_browser_removes_panel() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        assert!(mgr.close_browser("p1").is_ok());
        assert!(!mgr.has_panel("p1").unwrap());
    }

    #[test]
    fn close_browser_unknown_returns_error() {
        let mgr = new_manager();
        assert!(matches!(
            mgr.close_browser("nope").unwrap_err(),
            BrowserError::PanelNotFound(_)
        ));
    }

    #[test]
    fn panel_ids_returns_all_ids() {
        let mgr = new_manager();
        mgr.open_browser("a", "https://a.com").unwrap();
        mgr.open_browser("b", "https://b.com").unwrap();
        let mut ids = mgr.panel_ids().unwrap();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);
    }

    // -- Navigation ---------------------------------------------------------

    #[test]
    fn navigate_updates_url_and_loading() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        assert!(mgr.navigate("p1", "https://b.com").is_ok());
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://b.com");
        assert!(mgr.is_loading("p1").unwrap());
    }

    #[test]
    fn navigate_records_back_history() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        assert!(mgr.can_go_back("p1").unwrap());
    }

    #[test]
    fn navigate_clears_forward_history() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.go_back("p1").unwrap();
        // forward should now be available
        assert!(mgr.can_go_forward("p1").unwrap());
        // navigating to a new URL clears forward
        mgr.navigate("p1", "https://c.com").unwrap();
        assert!(!mgr.can_go_forward("p1").unwrap());
    }

    #[test]
    fn navigate_unknown_panel_returns_error() {
        let mgr = new_manager();
        assert!(matches!(
            mgr.navigate("nope", "https://a.com").unwrap_err(),
            BrowserError::PanelNotFound(_)
        ));
    }

    // -- Back / forward -----------------------------------------------------

    #[test]
    fn go_back_returns_previous_url() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        let target = mgr.go_back("p1").unwrap();
        assert_eq!(target, "https://a.com");
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://a.com");
    }

    #[test]
    fn go_back_then_forward_roundtrip() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.go_back("p1").unwrap();
        let target = mgr.go_forward("p1").unwrap();
        assert_eq!(target, "https://b.com");
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://b.com");
    }

    #[test]
    fn go_back_when_no_history_returns_error() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        assert!(matches!(
            mgr.go_back("p1").unwrap_err(),
            BrowserError::NoBackHistory(_)
        ));
    }

    #[test]
    fn go_forward_when_no_history_returns_error() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        assert!(matches!(
            mgr.go_forward("p1").unwrap_err(),
            BrowserError::NoForwardHistory(_)
        ));
    }

    // -- Reload -------------------------------------------------------------

    #[test]
    fn reload_sets_loading_state() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_loaded("p1").unwrap();
        assert!(!mgr.is_loading("p1").unwrap());
        mgr.reload("p1").unwrap();
        assert!(mgr.is_loading("p1").unwrap());
    }

    #[test]
    fn reload_preserves_url() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.reload("p1").unwrap();
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://a.com");
    }

    // -- Loading state ------------------------------------------------------

    #[test]
    fn set_loaded_transitions_to_idle() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        assert_eq!(mgr.loading_state("p1").unwrap(), LoadingState::Loading);
        mgr.set_loaded("p1").unwrap();
        assert_eq!(mgr.loading_state("p1").unwrap(), LoadingState::Idle);
    }

    #[test]
    fn set_load_failed_transitions_to_failed() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.set_load_failed("p1").unwrap();
        assert_eq!(mgr.loading_state("p1").unwrap(), LoadingState::Failed);
    }

    // -- Title updates ------------------------------------------------------

    #[test]
    fn set_title_updates_panel_title() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_title("p1", "My Page").unwrap();
        assert_eq!(mgr.get_title("p1").unwrap(), "My Page");
    }

    #[test]
    fn navigate_clears_title() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_title("p1", "Old Title").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        assert_eq!(mgr.get_title("p1").unwrap(), "");
    }

    // -- In-page URL update -------------------------------------------------

    #[test]
    fn set_url_updates_without_affecting_history() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_url("p1", "https://a.com/page2").unwrap();
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://a.com/page2");
        // No back history should have been created for in-page nav.
        assert!(!mgr.can_go_back("p1").unwrap());
    }

    // -- get_panel snapshot -------------------------------------------------

    #[test]
    fn get_panel_returns_snapshot() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        let panel = mgr.get_panel("p1").unwrap();
        assert_eq!(panel.id, "p1");
        assert_eq!(panel.current_url, "https://a.com");
    }

    // -- Shutdown -----------------------------------------------------------

    #[test]
    fn shutdown_clears_all_panels() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.open_browser("p2", "https://b.com").unwrap();
        mgr.shutdown();
        assert!(!mgr.has_panel("p1").unwrap());
        assert!(!mgr.has_panel("p2").unwrap());
        assert!(mgr.panel_ids().unwrap().is_empty());
    }

    // -- NavigationHistory unit tests ---------------------------------------

    #[test]
    fn history_trims_to_max_entries() {
        let mut history = NavigationHistory::new(3);
        for i in 0..5 {
            history.push_back(format!("https://a.com/{i}"));
        }
        assert_eq!(history.back_count(), 3);
    }

    #[test]
    fn history_forward_clears_on_new_navigation() {
        let mut history = NavigationHistory::new(10);
        history.push_back("https://a.com/1".to_string());
        history.push_back("https://a.com/2".to_string());
        // Simulate going back twice
        let _ = history.go_back("https://a.com/2".to_string());
        let _ = history.go_back("https://a.com/1".to_string());
        assert!(history.can_go_forward());
        // New navigation should clear forward
        history.push_back("https://c.com".to_string());
        assert!(!history.can_go_forward());
    }
}
