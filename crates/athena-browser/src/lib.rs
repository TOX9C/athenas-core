use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use thiserror::Error;
use url::Host;

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

/// Phase reported by Tauri's native page-load callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLoadPhase {
    Started,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavigationKind {
    Command,
    Native,
}

/// A navigation tracked by the backend. Native callbacks are accepted only
/// after `on_navigation` has observed their URL for this generation; this
/// prevents a delayed callback from an older navigation from mutating history.
#[derive(Debug, Clone)]
struct PendingNavigation {
    target_url: String,
    generation: u64,
    kind: NavigationKind,
    observed_urls: Vec<String>,
    started: bool,
    /// Whether this generation has already recorded its pre-navigation URL.
    /// Redirects stay in the same generation and must not create duplicate
    /// history entries.
    history_recorded: bool,
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
    /// Latest navigation awaiting native callbacks.
    #[serde(skip)]
    pending_navigation: Option<PendingNavigation>,
    /// Monotonic navigation generation used by native callback correlation.
    #[serde(skip)]
    navigation_generation: u64,
}

impl BrowserPanel {
    fn new(id: String, url: String) -> Self {
        Self {
            id,
            current_url: url.clone(),
            title: String::new(),
            history: NavigationHistory::default(),
            loading_state: LoadingState::Loading,
            pending_navigation: Some(PendingNavigation {
                target_url: url.clone(),
                generation: 1,
                kind: NavigationKind::Command,
                observed_urls: vec![url],
                started: false,
                history_recorded: false,
            }),
            navigation_generation: 1,
        }
    }
}

/// Stable state returned to the frontend. Internal history URLs stay in the
/// backend model instead of being serialized over IPC unnecessarily.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserSnapshot {
    pub id: String,
    pub current_url: String,
    pub title: String,
    pub loading_state: LoadingState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub generation: u64,
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Normalize a URL-bar entry into either an HTTP(S) URL or a Google search.
///
/// Bare hostnames such as `github.com` get an `https://` prefix. Bare words
/// and phrases such as `github` or `rust async book` become Google searches,
/// matching the behavior users expect from a browser address bar. Only
/// HTTP(S) destinations are accepted after normalization; credentials and
/// control characters are rejected so navigation cannot smuggle secrets or
/// non-web schemes into a child webview.
pub fn normalize_url(raw: &str) -> Result<String, BrowserError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BrowserError::InvalidUrl("URL is empty".to_string()));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(BrowserError::InvalidUrl(
            "URL contains control characters".to_string(),
        ));
    }

    let with_scheme = if let Some((scheme, _)) = trimmed.split_once("://") {
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return Err(BrowserError::InvalidUrl(
                "Only http:// and https:// URLs are allowed".to_string(),
            ));
        }
        trimmed.to_string()
    } else if let Some(parsed_bare_host) = parse_bare_host(trimmed) {
        let scheme = if is_local_host(&parsed_bare_host) {
            "http"
        } else {
            "https"
        };
        format!("{scheme}://{trimmed}")
    } else if trimmed.contains(':') {
        // A scheme-looking entry such as `javascript:...` or an unsupported
        // custom protocol must never be silently turned into a search.
        return Err(BrowserError::InvalidUrl(
            "Only http:// and https:// URLs are allowed".to_string(),
        ));
    } else {
        let encoded: String = url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect();
        format!("https://www.google.com/search?q={encoded}")
    };

    let parsed = url::Url::parse(&with_scheme)
        .map_err(|e| BrowserError::InvalidUrl(format!("invalid URL: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(BrowserError::InvalidUrl(
            "Only http:// and https:// URLs are allowed".to_string(),
        ));
    }
    if parsed.host_str().is_none() || parsed.username() != "" || parsed.password().is_some() {
        return Err(BrowserError::InvalidUrl(
            "URL must contain a host and no credentials".to_string(),
        ));
    }
    let valid_host = match parsed.host() {
        Some(Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost") || is_valid_hostname(host)
        }
        Some(Host::Ipv4(_address)) => true,
        Some(Host::Ipv6(_address)) => true,
        None => false,
    };
    if !valid_host {
        return Err(BrowserError::InvalidUrl(
            "URL lacks a valid domain".to_string(),
        ));
    }
    // Preserve the user's URL spelling/path while returning the validated
    // value; this keeps navigation history stable and avoids surprising slash
    // insertion for bare origins.
    Ok(with_scheme)
}

fn is_valid_hostname(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

fn is_local_host(url: &url::Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => {
            address.is_loopback() || address.is_private() || address.is_link_local()
        }
        Some(Host::Ipv6(address)) => {
            address.is_loopback() || (address.segments()[0] & 0xfe00) == 0xfc00
        }
        None => false,
    }
}

/// Parse a host-like address bar entry without mistaking a scheme-like value
/// such as `javascript:...` for a hostname. This deliberately accepts ports
/// and bracketed IPv6 literals.
fn parse_bare_host(value: &str) -> Option<url::Url> {
    if value.chars().any(char::is_whitespace) {
        return None;
    }
    let candidate = url::Url::parse(&format!("https://{value}")).ok()?;
    let valid_host = match candidate.host() {
        Some(Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost") || is_valid_hostname(host)
        }
        Some(Host::Ipv4(_)) | Some(Host::Ipv6(_)) => true,
        None => false,
    };
    valid_host.then_some(candidate)
}

fn urls_equivalent(left: &str, right: &str) -> bool {
    let Ok(left) = url::Url::parse(left) else {
        return left == right;
    };
    let Ok(right) = url::Url::parse(right) else {
        return false;
    };
    left.scheme().eq_ignore_ascii_case(right.scheme())
        && left.host() == right.host()
        && left.port_or_known_default() == right.port_or_known_default()
        && left.username() == right.username()
        && left.password() == right.password()
        && left.query() == right.query()
        && left.fragment() == right.fragment()
        && match (left.path(), right.path()) {
            ("", "/") | ("/", "") => true,
            (left_path, right_path) => left_path == right_path,
        }
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
    /// Serializes operations that must coordinate model state with a native
    /// child WebView handle in the Tauri command layer.
    operation_lock: Arc<std::sync::Mutex<()>>,
}

impl std::fmt::Debug for BrowserManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserManager")
            .field("panels", &"<RwLock<HashMap>>")
            .field("event_emitter", &"<Option>")
            .field("operation_lock", &"<Mutex>")
            .finish()
    }
}

impl Clone for BrowserManager {
    fn clone(&self) -> Self {
        Self {
            panels: Arc::clone(&self.panels),
            event_emitter: Arc::clone(&self.event_emitter),
            operation_lock: Arc::clone(&self.operation_lock),
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
            operation_lock: Arc::new(std::sync::Mutex::new(())),
        }
    }

    /// Serialize a command-layer operation that coordinates model and native
    /// child WebView state. Model callbacks intentionally do not take this
    /// guard, so native page events cannot deadlock an in-flight command.
    pub fn operation_guard(&self) -> Result<std::sync::MutexGuard<'_, ()>, BrowserError> {
        self.operation_lock
            .lock()
            .map_err(|_| BrowserError::LockPoison("browser operation".to_string()))
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
        // Event payloads may contain URLs with bearer tokens or private query
        // data. Never dump arbitrary browser event JSON into logs.
        log::debug!("[browser] event emitted: {}", channel);
    }

    fn emit_status(&self, id: &str, status: &str) {
        let (can_go_back, can_go_forward, generation) = self
            .read_lock()
            .ok()
            .and_then(|panels| {
                panels.get(id).map(|panel| {
                    (
                        panel.history.can_go_back(),
                        panel.history.can_go_forward(),
                        panel.navigation_generation,
                    )
                })
            })
            .unwrap_or((false, false, 0));
        self.emit_event(
            "browser:statusChange",
            &serde_json::json!({
                "id": id,
                "status": status,
                "canGoBack": can_go_back,
                "canGoForward": can_go_forward,
                "generation": generation,
            }),
        );
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

    fn begin_navigation(panel: &mut BrowserPanel, target_url: String, kind: NavigationKind) -> u64 {
        panel.navigation_generation = panel.navigation_generation.wrapping_add(1);
        let generation = panel.navigation_generation;
        panel.pending_navigation = Some(PendingNavigation {
            target_url: target_url.clone(),
            generation,
            kind,
            observed_urls: vec![target_url],
            started: false,
            history_recorded: false,
        });
        generation
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

        if !urls_equivalent(&panel.current_url, &normalized) && !panel.current_url.is_empty() {
            panel.history.push_back(panel.current_url.clone());
        }
        panel.current_url = normalized.clone();
        panel.loading_state = LoadingState::Loading;
        panel.title.clear();
        Self::begin_navigation(panel, normalized, NavigationKind::Command);
        let panel_id = id.to_string();
        drop(panels);
        self.emit_status(&panel_id, "loading");
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
        Self::begin_navigation(panel, target.clone(), NavigationKind::Command);
        let panel_id = id.to_string();
        drop(panels);
        self.emit_status(&panel_id, "loading");
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
        Self::begin_navigation(panel, target.clone(), NavigationKind::Command);
        let panel_id = id.to_string();
        drop(panels);
        self.emit_status(&panel_id, "loading");
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
        Self::begin_navigation(panel, panel.current_url.clone(), NavigationKind::Command);
        let panel_id = id.to_string();
        drop(panels);
        self.emit_status(&panel_id, "loading");
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

    /// Get a snapshot of the panel state without exposing internal history URLs.
    pub fn get_snapshot(&self, id: &str) -> Result<BrowserSnapshot, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(BrowserSnapshot {
            id: panel.id.clone(),
            current_url: panel.current_url.clone(),
            title: panel.title.clone(),
            loading_state: panel.loading_state,
            can_go_back: panel.history.can_go_back(),
            can_go_forward: panel.history.can_go_forward(),
            generation: panel.navigation_generation,
        })
    }

    /// Get a complete model snapshot for internal rollback and unit tests.
    pub fn get_panel(&self, id: &str) -> Result<BrowserPanel, BrowserError> {
        let panels = self.read_lock()?;
        let panel = panels
            .get(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        Ok(panel.clone())
    }

    /// Restore a complete model snapshot after a native operation fails and
    /// publish corrective events so the toolbar cannot retain optimistic state.
    pub fn restore_panel(&self, panel: BrowserPanel) -> Result<(), BrowserError> {
        let id = panel.id.clone();
        let url = panel.current_url.clone();
        let title = panel.title.clone();
        let loading_state = panel.loading_state;
        self.write_lock()?.insert(id.clone(), panel);

        self.emit_event(
            "browser:urlChange",
            &serde_json::json!({ "id": id.clone(), "url": url }),
        );
        self.emit_event(
            "browser:titleChange",
            &serde_json::json!({ "id": id.clone(), "title": title }),
        );
        self.emit_status(
            &id,
            match loading_state {
                LoadingState::Idle => "idle",
                LoadingState::Loading => "loading",
                LoadingState::Failed => "failed",
            },
        );
        Ok(())
    }

    // -- State updates (called by Tauri command layer / event handlers) ------

    /// Mark that a page has finished loading successfully.
    pub fn set_loaded(&self, id: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        panel.loading_state = LoadingState::Idle;
        panel.pending_navigation = None;
        let panel_id = id.to_string();
        drop(panels);

        self.emit_status(&panel_id, "idle");
        Ok(())
    }

    /// Mark that a page load has failed.
    pub fn set_load_failed(&self, id: &str) -> Result<(), BrowserError> {
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        panel.loading_state = LoadingState::Failed;
        panel.pending_navigation = None;
        let panel_id = id.to_string();
        drop(panels);

        self.emit_status(&panel_id, "failed");
        Ok(())
    }

    /// Record a native navigation request before its page-load callbacks fire.
    /// Tauri does not provide a stable native navigation ID, so the URL observed
    /// here is the correlation key for the current backend generation.
    pub fn observe_navigation(&self, id: &str, url: &str) -> Result<Option<u64>, BrowserError> {
        let normalized = normalize_url(url)?;
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        if let Some(pending) = panel.pending_navigation.as_mut() {
            if pending
                .observed_urls
                .iter()
                .any(|observed| urls_equivalent(observed, &normalized))
            {
                return Ok(Some(pending.generation));
            }
            // Before the active generation has started, a different URL is
            // most safely treated as a delayed callback from an older
            // navigation. Once it has started, a different URL is a redirect
            // (or an in-page navigation request) belonging to the same native
            // generation. Starting a second generation here would make stale
            // callbacks indistinguishable from legitimate redirects.
            if !pending.started && !urls_equivalent(&pending.target_url, &normalized) {
                return Ok(None);
            }
            pending.observed_urls.push(normalized);
            return Ok(Some(pending.generation));
        }
        Ok(Some(Self::begin_navigation(
            panel,
            normalized,
            NavigationKind::Native,
        )))
    }

    /// Apply a native page-load callback only when its URL was observed for the
    /// active navigation generation. Returns `Ok(None)` for stale callbacks.
    pub fn apply_page_load(
        &self,
        id: &str,
        url: &str,
        phase: PageLoadPhase,
    ) -> Result<Option<u64>, BrowserError> {
        let normalized = normalize_url(url)?;
        let mut panels = self.write_lock()?;
        let panel = panels
            .get_mut(id)
            .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
        let Some(pending) = panel.pending_navigation.as_mut() else {
            return Ok(None);
        };
        if !pending
            .observed_urls
            .iter()
            .any(|observed| urls_equivalent(observed, &normalized))
        {
            return Ok(None);
        }

        let generation = pending.generation;
        let kind = pending.kind;
        let was_started = pending.started;
        let target_matches = urls_equivalent(&pending.target_url, &normalized);
        let url_changed = !urls_equivalent(&panel.current_url, &normalized);
        if matches!(phase, PageLoadPhase::Started) {
            // A generation's first URL is the only URL allowed to mutate the
            // model during Started. Redirects are observed provisionally and
            // committed by a guarded Finished callback; this prevents a stale
            // post-start callback from overwriting a newer command's URL.
            if was_started && !target_matches {
                return Ok(Some(generation));
            }
            pending.started = true;
            if kind == NavigationKind::Native
                && url_changed
                && !pending.history_recorded
                && !panel.current_url.is_empty()
            {
                panel.history.push_back(panel.current_url.clone());
                pending.history_recorded = true;
            }
            panel.current_url = normalized.clone();
            panel.loading_state = LoadingState::Loading;
            let panel_id = id.to_string();
            drop(panels);
            if url_changed {
                self.emit_event(
                    "browser:urlChange",
                    &serde_json::json!({ "id": panel_id, "url": normalized }),
                );
            }
            self.emit_status(&panel_id, "loading");
            return Ok(Some(generation));
        }

        // A Finished callback can arrive without Started on some WebKit paths;
        // accept it if its URL was observed, while preserving the same history
        // rules and generation.
        if kind == NavigationKind::Native
            && !was_started
            && url_changed
            && !pending.history_recorded
            && !panel.current_url.is_empty()
        {
            panel.history.push_back(panel.current_url.clone());
            pending.history_recorded = true;
        }
        panel.current_url = normalized.clone();
        panel.loading_state = LoadingState::Idle;
        panel.pending_navigation = None;
        let panel_id = id.to_string();
        drop(panels);
        if url_changed && !was_started {
            self.emit_event(
                "browser:urlChange",
                &serde_json::json!({ "id": panel_id, "url": normalized }),
            );
        }
        self.emit_status(&panel_id, "idle");
        Ok(Some(generation))
    }

    /// Apply a page-load callback only when WebKit still reports the same
    /// committed URL. This extra check rejects a delayed Finished callback
    /// after a newer navigation has already committed.
    pub fn apply_page_load_for_current_url(
        &self,
        id: &str,
        callback_url: &str,
        committed_url: &str,
        phase: PageLoadPhase,
    ) -> Result<Option<u64>, BrowserError> {
        let callback_url = normalize_url(callback_url)?;
        let committed_url = normalize_url(committed_url)?;
        if !urls_equivalent(&callback_url, &committed_url) {
            return Ok(None);
        }
        self.apply_page_load(id, &callback_url, phase)
    }

    /// Compatibility helper for callers that report a completed native URL
    /// without separately forwarding `on_navigation` and load phases.
    pub fn set_url(&self, id: &str, url: &str) -> Result<(), BrowserError> {
        {
            let mut panels = self.write_lock()?;
            let panel = panels
                .get_mut(id)
                .ok_or_else(|| BrowserError::PanelNotFound(id.to_string()))?;
            panel.pending_navigation = None;
        }
        let _ = self.observe_navigation(id, url)?;
        self.apply_page_load(id, url, PageLoadPhase::Finished)?;
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
    fn normalize_url_turns_bare_word_into_google_search() {
        assert_eq!(
            normalize_url("github").unwrap(),
            "https://www.google.com/search?q=github"
        );
    }

    #[test]
    fn normalize_url_encodes_search_phrase() {
        assert_eq!(
            normalize_url("rust async book").unwrap(),
            "https://www.google.com/search?q=rust+async+book"
        );
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
    fn normalize_url_accepts_bare_host_ports() {
        assert_eq!(
            normalize_url("example.com:8443/docs").unwrap(),
            "https://example.com:8443/docs"
        );
        assert_eq!(
            normalize_url("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_url("127.0.0.1:5173").unwrap(),
            "http://127.0.0.1:5173"
        );
    }

    #[test]
    fn normalize_url_accepts_public_ipv6() {
        assert_eq!(
            normalize_url("http://[2001:db8::1]/docs").unwrap(),
            "http://[2001:db8::1]/docs"
        );
        assert_eq!(normalize_url("[::1]:3000").unwrap(), "http://[::1]:3000");
    }

    #[test]
    fn normalize_url_rejects_scheme_lookalikes() {
        assert!(normalize_url("javascript:alert(1)").is_err());
        assert!(normalize_url("example.com:bad-port").is_err());
    }

    #[test]
    fn normalize_url_uses_http_for_bare_localhost() {
        assert_eq!(normalize_url("localhost").unwrap(), "http://localhost");
        assert_eq!(
            normalize_url("192.168.1.20").unwrap(),
            "http://192.168.1.20"
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
        assert!(normalize_url("https://localhost-no-dot").is_err());
    }

    #[test]
    fn normalize_url_rejects_credentials() {
        assert!(normalize_url("https://user:password@example.com").is_err());
    }

    #[test]
    fn normalize_url_rejects_non_http_schemes() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
        ] {
            assert!(normalize_url(url).is_err(), "expected rejection: {url}");
        }
    }

    #[test]
    fn normalize_url_rejects_control_characters() {
        assert!(normalize_url("https://example.com/\nredirect").is_err());
    }

    #[test]
    fn normalize_url_rejects_malformed_hosts_and_ports() {
        for url in [
            "https://example..com",
            "https://example.com:bad",
            "https://.",
        ] {
            assert!(normalize_url(url).is_err(), "expected rejection: {url}");
        }
    }

    #[test]
    fn normalize_url_allows_localhost() {
        assert_eq!(
            normalize_url("http://localhost:3000").unwrap(),
            "http://localhost:3000"
        );
    }

    #[test]
    fn normalize_url_allows_ipv6_loopback() {
        assert_eq!(
            normalize_url("http://[::1]:3000").unwrap(),
            "http://[::1]:3000"
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
    fn navigate_same_url_does_not_duplicate_history() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://a.com/").unwrap();
        assert!(!mgr.can_go_back("p1").unwrap());
    }

    #[test]
    fn native_acknowledgement_does_not_duplicate_programmatic_navigation() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.set_url("p1", "https://b.com/").unwrap();
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://b.com/");
        assert_eq!(mgr.get_panel("p1").unwrap().history.back_count(), 1);
    }

    #[test]
    fn native_acknowledgement_preserves_forward_history_after_back() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.go_back("p1").unwrap();
        mgr.set_url("p1", "https://a.com/").unwrap();
        assert!(mgr.can_go_forward("p1").unwrap());
        assert_eq!(mgr.get_panel("p1").unwrap().history.back_count(), 0);
    }

    #[test]
    fn stale_page_load_callbacks_are_ignored_after_a_new_generation() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.observe_navigation("p1", "https://a.com").unwrap();
        mgr.apply_page_load("p1", "https://a.com", PageLoadPhase::Started)
            .unwrap();
        mgr.apply_page_load("p1", "https://a.com", PageLoadPhase::Finished)
            .unwrap();

        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.observe_navigation("p1", "https://b.com").unwrap();
        mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Started)
            .unwrap();
        mgr.navigate("p1", "https://c.com").unwrap();
        mgr.observe_navigation("p1", "https://c.com").unwrap();

        assert_eq!(mgr.observe_navigation("p1", "https://b.com").unwrap(), None);
        assert_eq!(
            mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Finished)
                .unwrap(),
            None
        );
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://c.com");
        assert!(mgr.is_loading("p1").unwrap());
    }

    #[test]
    fn delayed_navigation_observation_does_not_replace_started_generation() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_loaded("p1").unwrap();

        let first = mgr
            .observe_navigation("p1", "https://b.com")
            .unwrap()
            .unwrap();
        mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Started)
            .unwrap();
        assert_eq!(
            mgr.observe_navigation("p1", "https://c.com").unwrap(),
            Some(first)
        );
        mgr.apply_page_load("p1", "https://c.com", PageLoadPhase::Finished)
            .unwrap();

        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://c.com");
        assert_eq!(mgr.get_panel("p1").unwrap().history.back_count(), 1);
    }

    #[test]
    fn stale_post_start_started_callback_cannot_overwrite_newer_command() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_loaded("p1").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.observe_navigation("p1", "https://b.com").unwrap();
        mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Started)
            .unwrap();
        mgr.navigate("p1", "https://c.com").unwrap();
        mgr.observe_navigation("p1", "https://c.com").unwrap();
        mgr.apply_page_load("p1", "https://c.com", PageLoadPhase::Started)
            .unwrap();

        // A delayed callback from the old b.com load is observed but cannot
        // change the active c.com model state.
        mgr.observe_navigation("p1", "https://b.com").unwrap();
        mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Started)
            .unwrap();
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://c.com");
        assert!(mgr.is_loading("p1").unwrap());
    }

    #[test]
    fn committed_url_check_rejects_delayed_finished_callback() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_loaded("p1").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.observe_navigation("p1", "https://b.com").unwrap();
        mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Started)
            .unwrap();
        mgr.navigate("p1", "https://c.com").unwrap();
        mgr.observe_navigation("p1", "https://c.com").unwrap();

        assert_eq!(
            mgr.apply_page_load_for_current_url(
                "p1",
                "https://b.com",
                "https://c.com",
                PageLoadPhase::Finished,
            )
            .unwrap(),
            None
        );
        assert!(mgr.is_loading("p1").unwrap());
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://c.com");
    }

    #[test]
    fn redirect_callbacks_commit_to_the_active_command_generation() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.observe_navigation("p1", "https://a.com").unwrap();
        mgr.apply_page_load("p1", "https://a.com", PageLoadPhase::Started)
            .unwrap();
        mgr.apply_page_load("p1", "https://a.com", PageLoadPhase::Finished)
            .unwrap();

        mgr.navigate("p1", "https://b.com").unwrap();
        mgr.observe_navigation("p1", "https://b.com").unwrap();
        mgr.apply_page_load("p1", "https://b.com", PageLoadPhase::Started)
            .unwrap();
        mgr.observe_navigation("p1", "https://c.com").unwrap();
        mgr.apply_page_load("p1", "https://c.com", PageLoadPhase::Finished)
            .unwrap();

        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://c.com");
        // The redirect remains in the command generation, so it does not
        // create an extra history entry for the same navigation intent.
        assert_eq!(mgr.get_panel("p1").unwrap().history.back_count(), 1);
        assert!(!mgr.is_loading("p1").unwrap());
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
    fn set_url_records_native_navigation_in_history() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.set_url("p1", "https://a.com/page2").unwrap();
        assert_eq!(mgr.get_active_url("p1").unwrap(), "https://a.com/page2");
        assert!(mgr.can_go_back("p1").unwrap());
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

    #[test]
    fn get_snapshot_exposes_capabilities_without_history_urls() {
        let mgr = new_manager();
        mgr.open_browser("p1", "https://a.com").unwrap();
        mgr.navigate("p1", "https://b.com").unwrap();
        let snapshot = mgr.get_snapshot("p1").unwrap();
        assert_eq!(snapshot.current_url, "https://b.com");
        assert!(snapshot.can_go_back);
        assert!(!snapshot.can_go_forward);
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("https://a.com"));
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
