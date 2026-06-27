use dioxus::prelude::*;
use std::collections::HashMap;

fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single line of agent output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputLine {
    pub pane_id: String,
    pub line_num: usize,
    pub timestamp: i64,
    pub text: String,
    /// Precomputed stderr heuristic. Set once at line arrival; never
    /// re-evaluated during render. See [`is_stderr_like`].
    pub is_stderr: bool,
}

/// Heuristic: text containing error/warn/fail/exception keywords looks like
/// stderr output. Called once per line at construction time (in the IPC
/// listener), never per render.
///
/// Returns the same answer as the legacy render-time heuristic. Allocates a
/// single lowercased copy of `text` per call — at most once per incoming line.
pub fn is_stderr_like(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("error")
        || lower.contains("warn")
        || lower.contains("fail")
        || lower.contains("exception")
}

/// Metadata about an agent's output stream.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentOutputInfo {
    pub pane_id: String,
    pub agent_type: String,
    pub line_count: usize,
    pub created_at: i64,
    pub last_activity_at: i64,
}

/// State of a subscription to a pane's output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubscriptionState {
    pub subscription_id: Option<String>,
    pub pane_id: Option<String>,
    pub active: bool,
}

/// Maximum lines retained per output buffer.
const MAX_LINES_PER_BUFFER: usize = 5000;
/// Maximum characters retained per output line (prevents DoS from huge single lines).
const MAX_TEXT_LENGTH: usize = 10000;
/// Maximum number of tracked output panes.
const MAX_PANE_COUNT: usize = 100;
/// Target pane count after garbage collection.
const PANE_GC_TARGET: usize = 80;
/// Threshold for idle-pane GC (milliseconds). A pane whose buffer has not been
/// touched for this duration is eligible for eviction.
const PANE_GC_IDLE_THRESHOLD_MS: u64 = 30 * 60 * 1000;
/// Recommended sweep interval for periodic GC (milliseconds).
pub const PANE_GC_INTERVAL_MS: u64 = 5 * 60 * 1000;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global agent output tracking state.
#[derive(Clone, PartialEq)]
pub struct AgentOutputState {
    pub buffers: Vec<(String, Vec<OutputLine>)>,
    pub agents: Vec<AgentOutputInfo>,
    pub selected_pane_id: Option<String>,
    pub subscription: SubscriptionState,
    pub inspector_open: bool,
    pub auto_scroll: bool,
    /// Last time each pane's buffer was read/written (epoch ms). Used by `gc()` to evict
    /// panes that have been idle longer than `PANE_GC_IDLE_THRESHOLD_MS`.
    last_access: HashMap<String, u64>,
}

impl Default for AgentOutputState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentOutputState {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            agents: Vec::new(),
            selected_pane_id: None,
            subscription: SubscriptionState {
                subscription_id: None,
                pane_id: None,
                active: false,
            },
            inspector_open: false,
            auto_scroll: true,
            last_access: HashMap::new(),
        }
    }

    // -- Helpers -----------------------------------------------------------

    fn find_buffer_mut(&mut self, pane_id: &str) -> Option<&mut Vec<OutputLine>> {
        self.buffers
            .iter_mut()
            .find(|(id, _)| id == pane_id)
            .map(|(_, lines)| lines)
    }

    fn trim_lines(lines: &mut Vec<OutputLine>) {
        if lines.len() > MAX_LINES_PER_BUFFER {
            let excess = lines.len() - MAX_LINES_PER_BUFFER;
            lines.drain(0..excess);
        }
    }

    fn update_last_activity(&mut self, pane_id: &str, timestamp: i64) {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.pane_id == pane_id) {
            agent.last_activity_at = timestamp;
        }
    }

    /// Mark a pane as recently used. Call on every buffer mutation so that
    /// the idle-based `gc()` doesn't evict an active pane.
    fn touch(&mut self, pane_id: &str) {
        self.last_access.insert(pane_id.to_string(), now_ms());
    }

    fn maybe_gc_panes(&mut self) {
        if self.agents.len() <= MAX_PANE_COUNT {
            return;
        }
        let mut activity: Vec<(i64, String)> = self
            .agents
            .iter()
            .map(|a| (a.last_activity_at, a.pane_id.clone()))
            .collect();
        activity.sort_by_key(|x| x.0);
        let to_remove = self.agents.len().saturating_sub(PANE_GC_TARGET);
        for (_, pane_id) in activity.into_iter().take(to_remove) {
            self.unregister_pane(&pane_id);
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn set_lines(&mut self, pane_id: impl Into<String>, lines: Vec<OutputLine>) {
        let key = pane_id.into();
        let mut trimmed = lines;
        // Defensive: per-line text length cap (set_lines may be called with
        // a fresh batch from the backend that hasn't gone through append_line).
        for line in trimmed.iter_mut() {
            if line.text.len() > MAX_TEXT_LENGTH {
                line.text.truncate(MAX_TEXT_LENGTH);
            }
        }
        Self::trim_lines(&mut trimmed);
        if let Some(buf) = self.find_buffer_mut(&key) {
            *buf = trimmed;
        } else {
            self.buffers.push((key.clone(), trimmed));
            self.maybe_gc_panes();
        }
        self.touch(&key);
    }

    pub fn append_line(&mut self, mut line: OutputLine) {
        let pane_id = line.pane_id.clone();
        // Truncate overly long lines to prevent memory spikes
        if line.text.len() > MAX_TEXT_LENGTH {
            line.text.truncate(MAX_TEXT_LENGTH);
        }
        if let Some(buf) = self.find_buffer_mut(&pane_id) {
            buf.push(line);
            Self::trim_lines(buf);
        } else {
            self.buffers.push((pane_id.clone(), vec![line]));
            self.maybe_gc_panes();
        }
        // Update last activity for GC sorting
        let timestamp = chrono::Utc::now().timestamp();
        self.update_last_activity(&pane_id, timestamp);
        self.touch(&pane_id);
    }

    pub fn clear_buffer(&mut self, pane_id: &str) {
        self.buffers.retain(|(id, _)| id != pane_id);
        // Clear the access stamp so the idle GC doesn't keep a phantom entry
        // around pointing at a non-existent buffer.
        self.last_access.remove(pane_id);
    }

    pub fn set_agents(&mut self, agents: Vec<AgentOutputInfo>) {
        self.agents = agents;
    }

    pub fn select_agent(&mut self, pane_id: Option<String>) {
        self.selected_pane_id = pane_id;
    }

    pub fn set_subscription(&mut self, sub: SubscriptionState) {
        self.subscription = sub;
    }

    pub fn clear_subscription(&mut self) {
        self.subscription = SubscriptionState {
            subscription_id: None,
            pane_id: None,
            active: false,
        };
    }

    pub fn set_inspector_open(&mut self, open: bool) {
        self.inspector_open = open;
    }

    pub fn set_auto_scroll(&mut self, auto: bool) {
        self.auto_scroll = auto;
    }

    // -- Garbage collection -------------------------------------------------

    /// Evict panes (and their buffers) that have not been touched within
    /// `PANE_GC_IDLE_THRESHOLD`. Safe to call periodically (every
    /// `PANE_GC_INTERVAL` from a `use_effect`).
    ///
    /// This complements `maybe_gc_panes` (a hard cap on the total number of
    /// panes) by removing panes that exist *below* that cap but are no longer
    /// receiving output — e.g. an agent pane whose subscription ended without
    /// an explicit `unregister_pane` event.
    pub fn gc(&mut self) {
        let now = now_ms();
        // First pass: collect stale pane ids.
        let stale: Vec<String> = self
            .last_access
            .iter()
            .filter_map(|(pane_id, t)| {
                if now - *t >= PANE_GC_IDLE_THRESHOLD_MS {
                    Some(pane_id.clone())
                } else {
                    None
                }
            })
            .collect();

        for pane_id in &stale {
            self.unregister_pane(pane_id);
        }
    }

    // -- Event handlers for Tauri push events -------------------------------

    /// Register a new output pane.
    pub fn register_pane(&mut self, pane_id: String, agent_type: String, now: i64) {
        if !self.agents.iter().any(|a| a.pane_id == pane_id) {
            self.agents.push(AgentOutputInfo {
                pane_id: pane_id.clone(),
                agent_type,
                line_count: 0,
                created_at: now,
                last_activity_at: now,
            });
            // Ensure buffer exists
            if !self.buffers.iter().any(|(id, _)| id == &pane_id) {
                self.buffers.push((pane_id.clone(), Vec::new()));
            }
            self.maybe_gc_panes();
        }
        self.touch(&pane_id);
    }

    /// Unregister an output pane.
    pub fn unregister_pane(&mut self, pane_id: &str) {
        self.agents.retain(|a| a.pane_id != pane_id);
        self.buffers.retain(|(id, _)| id != pane_id);
        if self.selected_pane_id.as_deref() == Some(pane_id) {
            self.selected_pane_id = None;
        }
        self.last_access.remove(pane_id);
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the agent output signal from the Dioxus context.
pub fn use_agent_output_store() -> Signal<AgentOutputState> {
    use_context::<Signal<AgentOutputState>>()
}

/// Initialize the agent output store as a context provider.
pub fn provide_agent_output_store() {
    use_context_provider(|| Signal::new(AgentOutputState::new()));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_line(pane: &str, text: &str) -> OutputLine {
        OutputLine {
            pane_id: pane.to_string(),
            line_num: 0,
            timestamp: 0,
            text: text.to_string(),
            is_stderr: false,
        }
    }

    #[test]
    fn gc_removes_idle_panes_and_keeps_active() {
        let mut state = AgentOutputState::new();
        state.register_pane("pane-1".to_string(), "claude".to_string(), 1_000);
        state.register_pane("pane-2".to_string(), "shell".to_string(), 1_000);
        state.append_line(make_line("pane-1", "hello"));
        state.append_line(make_line("pane-2", "world"));

        assert_eq!(state.buffers.len(), 2);
        assert_eq!(state.agents.len(), 2);

        // Backdate pane-1 so it appears 31 minutes idle.
        let stale = now_ms() - (PANE_GC_IDLE_THRESHOLD_MS + 60_000);
        state.last_access.insert("pane-1".to_string(), stale);

        state.gc();

        // pane-1 evicted everywhere
        assert!(
            !state.buffers.iter().any(|(id, _)| id == "pane-1"),
            "stale buffer should be evicted"
        );
        assert!(
            !state.agents.iter().any(|a| a.pane_id == "pane-1"),
            "stale agent should be evicted"
        );
        assert!(
            !state.last_access.contains_key("pane-1"),
            "stale last_access should be removed"
        );

        // pane-2 retained
        assert!(
            state.buffers.iter().any(|(id, _)| id == "pane-2"),
            "active buffer should remain"
        );
        assert!(
            state.agents.iter().any(|a| a.pane_id == "pane-2"),
            "active agent should remain"
        );
        assert!(
            state.last_access.contains_key("pane-2"),
            "active last_access should remain"
        );
    }

    #[test]
    fn append_line_truncates_oversized_text() {
        let mut state = AgentOutputState::new();
        state.register_pane("pane-x".to_string(), "shell".to_string(), 1_000);
        let huge = "a".repeat(MAX_TEXT_LENGTH + 100);
        state.append_line(make_line("pane-x", &huge));
        let (_, lines) = state
            .buffers
            .iter()
            .find(|(id, _)| id == "pane-x")
            .expect("buffer exists");
        assert_eq!(lines[0].text.len(), MAX_TEXT_LENGTH);
    }

    #[test]
    fn touch_resets_idle_clock() {
        let mut state = AgentOutputState::new();
        state.register_pane("pane-y".to_string(), "claude".to_string(), 1_000);
        // Backdate
        let stale = now_ms() - (PANE_GC_IDLE_THRESHOLD_MS + 60_000);
        state.last_access.insert("pane-y".to_string(), stale);

        // A subsequent append should refresh the timestamp.
        state.append_line(make_line("pane-y", "ping"));
        state.gc();
        assert!(state.buffers.iter().any(|(id, _)| id == "pane-y"));
    }

    #[test]
    fn unregister_clears_last_access() {
        let mut state = AgentOutputState::new();
        state.register_pane("pane-z".to_string(), "shell".to_string(), 1_000);
        assert!(state.last_access.contains_key("pane-z"));
        state.unregister_pane("pane-z");
        assert!(!state.last_access.contains_key("pane-z"));
    }
}
