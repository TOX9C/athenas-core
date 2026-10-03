use crate::utils::time::now_ms;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[path = "agent_output_model.rs"]
mod agent_output_model;

pub use agent_output_model::{
    is_stderr_like, AgentOutputInfo, OutputLine, SubscriptionState, PANE_GC_INTERVAL_MS,
};
use agent_output_model::{
    MAX_LINES_PER_BUFFER, MAX_PANE_COUNT, MAX_TEXT_LENGTH, PANE_GC_IDLE_THRESHOLD_MS,
    PANE_GC_TARGET,
};

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Per-pane agent output store, mirroring the `TerminalRegistry` pattern.
///
/// Previously a single `Signal<AgentOutputState>` held every pane's buffer;
/// a batch for pane A re-validated (and re-rendered) every component
/// subscribed anywhere in the store — the output panel, the per-pane status
/// bars, the selector — on every batch. Now each pane's lines live in a
/// lazily-created `Signal<Vec<OutputLine>>` keyed in a `HashMap` (O(1) —
/// `append_batch`'s `Vec::find` is gone), so a write to pane A's signal
/// invalidates only pane A's subscribers (the output panel when it selects
/// pane A, pane A's status bar).
///
/// Membership (`agents`), selection, subscription, and UI flags are small
/// separate signals: a batch append touches only the target buffer signal
/// and the non-reactive `last_access` map, so list subscribers (agent
/// selector) do not re-render at event rate.
#[derive(Clone)]
pub struct AgentOutputStore {
    buffers: Rc<RefCell<HashMap<String, Signal<Vec<OutputLine>>>>>,
    agents: Signal<Vec<AgentOutputInfo>>,
    selected_pane_id: Signal<Option<String>>,
    subscription: Signal<SubscriptionState>,
    inspector_open: Signal<bool>,
    auto_scroll: Signal<bool>,
    /// Last wall-clock access per pane (epoch ms) for the idle `gc()` sweep.
    /// Non-reactive by design: touching it per batch must not re-render.
    last_access: Rc<RefCell<HashMap<String, u64>>>,
}

impl Default for AgentOutputStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentOutputStore {
    pub fn new() -> Self {
        Self {
            buffers: Rc::new(RefCell::new(HashMap::new())),
            agents: Signal::new(Vec::new()),
            selected_pane_id: Signal::new(None),
            subscription: Signal::new(SubscriptionState::default()),
            inspector_open: Signal::new(false),
            auto_scroll: Signal::new(true),
            last_access: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    // -- Signal accessors (subscribe to exactly one concern) ---------------

    /// Reactive membership list. Buffers change at event rate; this changes
    /// only on register/unregister, so list views subscribe here.
    pub fn agents_signal(&self) -> Signal<Vec<AgentOutputInfo>> {
        self.agents
    }

    pub fn selected_pane_id_signal(&self) -> Signal<Option<String>> {
        self.selected_pane_id
    }

    pub fn subscription_signal(&self) -> Signal<SubscriptionState> {
        self.subscription
    }

    pub fn inspector_open_signal(&self) -> Signal<bool> {
        self.inspector_open
    }

    pub fn auto_scroll_signal(&self) -> Signal<bool> {
        self.auto_scroll
    }

    /// The reactive per-pane line buffer, or `None` if the pane is unknown.
    pub fn buffer_signal(&self, pane_id: &str) -> Option<Signal<Vec<OutputLine>>> {
        self.buffers.borrow().get(pane_id).cloned()
    }

    /// Read a pane's buffer length without subscribing.
    pub fn buffer_len(&self, pane_id: &str) -> usize {
        self.buffer_signal(pane_id)
            .map(|s| s.peek().len())
            .unwrap_or(0)
    }

    // -- Mutators -----------------------------------------------------------

    /// Truncate an over-long line on a char boundary: `String::truncate`
    /// panics when the byte index lands mid-codepoint, and agent output
    /// routinely contains emoji/CJK.
    fn cap_line_text(line: &mut OutputLine) {
        if line.text.len() > MAX_TEXT_LENGTH {
            let mut end = MAX_TEXT_LENGTH;
            while !line.text.is_char_boundary(end) {
                end -= 1;
            }
            let mut text = line.text.to_string();
            text.truncate(end);
            line.text = Rc::from(text.as_str());
        }
    }

    fn trim_lines(lines: &mut Vec<OutputLine>) {
        if lines.len() > MAX_LINES_PER_BUFFER {
            let excess = lines.len() - MAX_LINES_PER_BUFFER;
            lines.drain(0..excess);
        }
    }

    /// Mark a pane as recently used so the idle `gc()` doesn't evict an
    /// active pane. Non-reactive (see the field docs).
    fn touch(&self, pane_id: &str) {
        self.last_access
            .borrow_mut()
            .insert(pane_id.to_string(), now_ms());
    }

    fn maybe_gc_panes(&self) {
        if self.agents.peek().len() <= MAX_PANE_COUNT {
            return;
        }
        let access = self.last_access.borrow();
        let mut activity: Vec<(u64, String)> = self
            .agents
            .peek()
            .iter()
            .map(|a| (access.get(&a.pane_id).copied().unwrap_or(0), a.pane_id.clone()))
            .collect();
        activity.sort_by_key(|x| x.0);
        drop(access);
        let to_remove = activity.len().saturating_sub(PANE_GC_TARGET);
        for (_, pane_id) in activity.into_iter().take(to_remove) {
            self.unregister_pane(&pane_id);
        }
    }

    /// Append one output line while preserving the single-line API used by
    /// existing callers.
    pub fn append_line(&self, line: OutputLine) {
        let pane_id = line.pane_id.clone();
        self.append_batch(&pane_id, vec![line]);
    }

    /// Append a backend batch for a known pane in a single write to that
    /// pane's own signal — only pane-A subscribers re-render for pane-A data.
    ///
    /// The explicit pane id lets the hot path reuse the listener's existing
    /// `Vec` without collecting it again. Malformed lines for another pane are
    /// discarded instead of being silently attached to the first pane.
    pub fn append_batch(&self, pane_id: &str, mut lines: Vec<OutputLine>) {
        lines.retain(|line| line.pane_id == pane_id);
        if lines.is_empty() {
            return;
        }

        // Truncate overly long lines before they enter the retained buffer.
        for line in &mut lines {
            Self::cap_line_text(line);
        }

        if let Some(signal) = self.buffer_signal(pane_id) {
            let mut guard = signal.write_unchecked();
            guard.extend(lines);
            Self::trim_lines(&mut guard);
        } else {
            Self::trim_lines(&mut lines);
            self.buffers.borrow_mut().insert(
                pane_id.to_string(),
                Signal::new_in_scope(lines, ScopeId::APP),
            );
            self.maybe_gc_panes();
        }

        self.touch(pane_id);
    }

    /// Generic convenience wrapper for callers that do not already own a
    /// backend batch. The event bus should prefer `append_batch` to avoid the
    /// extra collection.
    pub fn append_lines<I>(&self, lines: I)
    where
        I: IntoIterator<Item = OutputLine>,
    {
        let lines = lines.into_iter().collect::<Vec<_>>();
        let Some(pane_id) = lines.first().map(|line| line.pane_id.clone()) else {
            return;
        };
        self.append_batch(&pane_id, lines);
    }

    pub fn clear_buffer(&self, pane_id: &str) {
        if let Some(signal) = self.buffer_signal(pane_id) {
            signal.write_unchecked().clear();
        }
        // Clear the access stamp so the idle GC doesn't keep a phantom entry
        // around pointing at a non-existent buffer.
        self.last_access.borrow_mut().remove(pane_id);
    }

    /// Seed a pane's buffer from the backend history tail (Agent Inspector
    /// backfill on open / pane switch). Capture is gated on inspector
    /// visibility, so lines emitted while it was closed never reached this
    /// buffer; the backend `OutputBuffer` is always maintained, so its tail
    /// is the authoritative history.
    ///
    /// Prepend-only by `line_num`: batches can land between the async fetch
    /// and this call, so only lines strictly older than the buffer's current
    /// first line are inserted — nothing already present is overwritten or
    /// reordered.
    pub fn seed_history(&self, pane_id: &str, mut lines: Vec<OutputLine>) {
        let existing_first = self
            .buffer_signal(pane_id)
            .and_then(|signal| signal.peek().first().map(|l| l.line_num));
        if let Some(first) = existing_first {
            lines.retain(|line| line.line_num < first);
        }
        if lines.is_empty() {
            return;
        }

        for line in &mut lines {
            Self::cap_line_text(line);
        }

        if let Some(signal) = self.buffer_signal(pane_id) {
            let mut guard = signal.write_unchecked();
            let mut merged = std::mem::take(&mut *guard);
            lines.append(&mut merged);
            *guard = lines;
            // Trimming drops the oldest (seeded) lines first — correct.
            Self::trim_lines(&mut guard);
        } else {
            Self::trim_lines(&mut lines);
            self.buffers.borrow_mut().insert(
                pane_id.to_string(),
                Signal::new_in_scope(lines, ScopeId::APP),
            );
            self.maybe_gc_panes();
        }

        self.touch(pane_id);
    }

    pub fn set_agents(&self, agents: Vec<AgentOutputInfo>) {
        *self.agents.write_unchecked() = agents;
    }

    pub fn select_agent(&self, pane_id: Option<String>) {
        *self.selected_pane_id.write_unchecked() = pane_id;
    }

    pub fn set_subscription(&self, sub: SubscriptionState) {
        *self.subscription.write_unchecked() = sub;
    }

    pub fn clear_subscription(&self) {
        *self.subscription.write_unchecked() = SubscriptionState::default();
    }

    pub fn set_inspector_open(&self, open: bool) {
        *self.inspector_open.write_unchecked() = open;
    }

    /// True when any consumer reads captured agent output — currently the
    /// Agent Inspector being open. Non-reactive peek for hot event paths:
    /// the `output-capture:batch` listener uses it to skip the JSON walk and
    /// buffer writes entirely when nobody is watching.
    pub fn capture_wanted(&self) -> bool {
        *self.inspector_open.peek()
    }

    pub fn set_auto_scroll(&self, auto: bool) {
        *self.auto_scroll.write_unchecked() = auto;
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
    pub fn gc(&self) {
        let now = now_ms();
        let stale: Vec<String> = self
            .last_access
            .borrow()
            .iter()
            .filter_map(|(pane_id, t)| {
                // saturating_sub: a step back of the clock must not panic
                // debug builds on underflow.
                if now.saturating_sub(*t) >= PANE_GC_IDLE_THRESHOLD_MS {
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
    pub fn register_pane(&self, pane_id: String, agent_type: String, now: i64) {
        if !self.agents.peek().iter().any(|a| a.pane_id == pane_id) {
            self.agents.write_unchecked().push(AgentOutputInfo {
                pane_id: pane_id.clone(),
                agent_type,
                line_count: 0,
                created_at: now,
                last_activity_at: now,
            });
            // Ensure buffer exists.
            if !self.buffers.borrow().contains_key(&pane_id) {
                self.buffers.borrow_mut().insert(
                    pane_id.clone(),
                    Signal::new_in_scope(Vec::new(), ScopeId::APP),
                );
            }
            self.maybe_gc_panes();
        }
        self.touch(&pane_id);
    }

    /// Unregister an output pane.
    pub fn unregister_pane(&self, pane_id: &str) {
        self.agents.write_unchecked().retain(|a| a.pane_id != pane_id);
        self.buffers.borrow_mut().remove(pane_id);
        if self.selected_pane_id.peek().as_deref() == Some(pane_id) {
            *self.selected_pane_id.write_unchecked() = None;
        }
        self.last_access.borrow_mut().remove(pane_id);
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the agent output store from the Dioxus context.
pub fn use_agent_output_store() -> AgentOutputStore {
    use_context::<AgentOutputStore>()
}

/// Initialize the agent output store as a context provider.
pub fn provide_agent_output_store() {
    use_context_provider(AgentOutputStore::new);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::VirtualDom;

    fn make_line(pane: &str, text: &str) -> OutputLine {
        OutputLine {
            pane_id: pane.to_string(),
            line_num: 0,
            timestamp: 0,
            text: Rc::from(text),
            is_stderr: false,
        }
    }

    // Per-test body stashed in a thread-local so the root component closure —
    // which `VirtualDom::new` requires as a non-capturing `fn()` pointer — can
    // still access it. Dioxus signals panic outside a live runtime.
    thread_local! {
        static PENDING_BODY:
            std::cell::RefCell<Option<Box<dyn FnOnce(&AgentOutputStore)>>> =
            const { std::cell::RefCell::new(None) };
    }

    fn run_in_dom(body: impl FnOnce(&AgentOutputStore) + 'static) {
        PENDING_BODY.with(|cell| cell.replace(Some(Box::new(body))));
        let mut dom = VirtualDom::new(|| {
            let store: AgentOutputStore = use_context_provider(AgentOutputStore::new);
            PENDING_BODY.with(|cell| {
                if let Some(b) = cell.borrow_mut().take() {
                    b(&store);
                }
            });
            rsx! {}
        });
        dom.rebuild_to_vec();
    }

    #[test]
    fn gc_removes_idle_panes_and_keeps_active() {
        run_in_dom(|store| {
            store.register_pane("pane-1".to_string(), "claude".to_string(), 1_000);
            store.register_pane("pane-2".to_string(), "shell".to_string(), 1_000);
            store.append_line(make_line("pane-1", "hello"));
            store.append_line(make_line("pane-2", "world"));

            assert_eq!(store.buffer_len("pane-1"), 1);
            assert_eq!(store.agents.peek().len(), 2);

            // Backdate pane-1 so it appears 31 minutes idle.
            let stale = now_ms() - (PANE_GC_IDLE_THRESHOLD_MS + 60_000);
            store.last_access.borrow_mut().insert("pane-1".to_string(), stale);

            store.gc();

            assert!(
                store.buffer_signal("pane-1").is_none(),
                "stale buffer should be evicted"
            );
            assert!(
                !store.agents.peek().iter().any(|a| a.pane_id == "pane-1"),
                "stale agent should be evicted"
            );
            assert!(
                !store.last_access.borrow().contains_key("pane-1"),
                "stale last_access should be removed"
            );

            assert!(
                store.buffer_signal("pane-2").is_some(),
                "active buffer should remain"
            );
            assert!(
                store.agents.peek().iter().any(|a| a.pane_id == "pane-2"),
                "active agent should remain"
            );
        });
    }

    #[test]
    fn append_line_truncates_oversized_text() {
        run_in_dom(|store| {
            store.register_pane("pane-x".to_string(), "shell".to_string(), 1_000);
            let huge = "a".repeat(MAX_TEXT_LENGTH + 100);
            store.append_line(make_line("pane-x", &huge));
            let lines = store.buffer_signal("pane-x").expect("buffer exists");
            assert_eq!(lines.peek()[0].text.len(), MAX_TEXT_LENGTH);
        });
    }

    #[test]
    fn append_lines_preserves_order_in_one_batch() {
        run_in_dom(|store| {
            store.register_pane("pane-batch".to_string(), "claude".to_string(), 1_000);
            store.append_batch(
                "pane-batch",
                vec![
                    make_line("pane-batch", "first"),
                    make_line("pane-batch", "second"),
                    make_line("pane-batch", "third"),
                ],
            );

            let lines = store.buffer_signal("pane-batch").expect("batch buffer exists");
            assert_eq!(
                lines
                    .peek()
                    .iter()
                    .map(|line| line.text.as_ref())
                    .collect::<Vec<_>>(),
                ["first", "second", "third"]
            );
        });
    }

    #[test]
    fn append_lines_trims_once_at_buffer_limit() {
        run_in_dom(|store| {
            store.register_pane("pane-limit".to_string(), "shell".to_string(), 1_000);
            store.append_batch(
                "pane-limit",
                (0..(MAX_LINES_PER_BUFFER + 3))
                    .map(|index| make_line("pane-limit", &format!("line-{index}")))
                    .collect(),
            );

            let lines = store.buffer_signal("pane-limit").expect("limited buffer exists");
            let guard = lines.peek();
            assert_eq!(guard.len(), MAX_LINES_PER_BUFFER);
            assert_eq!(guard.first().map(|line| line.text.as_ref()), Some("line-3"));
            assert_eq!(
                guard.last().map(|line| line.text.as_ref()),
                Some("line-5002")
            );
        });
    }

    #[test]
    fn append_batch_ignores_empty_and_mixed_pane_lines() {
        run_in_dom(|store| {
            store.append_batch("pane-empty", Vec::new());
            store.append_batch(
                "pane-a",
                vec![
                    make_line("pane-a", "kept"),
                    make_line("pane-b", "discarded"),
                ],
            );

            let lines = store
                .buffer_signal("pane-a")
                .expect("valid batch line should create a buffer");
            assert_eq!(lines.peek().len(), 1);
            assert_eq!(lines.peek()[0].text.as_ref(), "kept");
        });
    }

    #[test]
    fn touch_resets_idle_clock() {
        run_in_dom(|store| {
            store.register_pane("pane-y".to_string(), "claude".to_string(), 1_000);
            // Backdate
            let stale = now_ms() - (PANE_GC_IDLE_THRESHOLD_MS + 60_000);
            store.last_access.borrow_mut().insert("pane-y".to_string(), stale);

            // A subsequent append should refresh the timestamp.
            store.append_line(make_line("pane-y", "ping"));
            store.gc();
            assert!(store.buffer_signal("pane-y").is_some());
        });
    }

    #[test]
    fn unregister_clears_last_access() {
        run_in_dom(|store| {
            store.register_pane("pane-z".to_string(), "shell".to_string(), 1_000);
            assert!(store.last_access.borrow().contains_key("pane-z"));
            store.unregister_pane("pane-z");
            assert!(!store.last_access.borrow().contains_key("pane-z"));
        });
    }

    #[test]
    fn select_agent_survives_buffer_appends() {
        run_in_dom(|store| {
            store.select_agent(Some("pane-a".to_string()));
            store.append_line(make_line("pane-a", "data"));
            assert_eq!(store.selected_pane_id.peek().as_deref(), Some("pane-a"));
        });
    }
}
