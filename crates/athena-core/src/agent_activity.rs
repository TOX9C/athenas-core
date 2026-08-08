//! Agent activity tracker: a backend-owned, per-pane state machine that
//! derives live agent status from PTY output, foreground-process lifecycle,
//! shell-integration events, and per-agent session files.
//!
//! Design rules (see docs/plans/agent-activity-notifications.md):
//! - **Plugin-status-wins**: panes with a connected plugin-host session skip
//!   all heuristics; the plugin drives their status (the state.rs adapter
//!   translates `agents:*` events to add `paneId`).
//! - **"Finished" requires a positive signal**: the agent process exiting
//!   (foreground returns to shell) or the shell reporting the agent's launch
//!   command finished. Output silence alone only moves the badge to idle.
//! - **Notifications fire on transitions only**, with per-pane cooldown.

use crate::agent_detection::{agent_label, HistorySnapshot};
use crate::notification::{NotificationEvent, NotificationService, NotificationType};
use crate::EventEmitter;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Milliseconds since UNIX epoch — local helper (other modules' `now_ms` are
/// `pub(super)`-scoped and not importable here).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Mirrors the frontend `AgentRunStatus` string values so the existing
/// `agent:status` bus mapping works unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentActivityStatus {
    Idle,
    Thinking,
    Working,
    WaitingForInput,
    Completed,
    Error,
    Cancelled,
    Disconnected,
}

impl AgentActivityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentActivityStatus::Idle => "idle",
            AgentActivityStatus::Thinking => "thinking",
            AgentActivityStatus::Working => "working",
            AgentActivityStatus::WaitingForInput => "waiting_for_input",
            AgentActivityStatus::Completed => "completed",
            AgentActivityStatus::Error => "error",
            AgentActivityStatus::Cancelled => "cancelled",
            AgentActivityStatus::Disconnected => "disconnected",
        }
    }

    pub fn from_str(s: &str) -> AgentActivityStatus {
        match s {
            "thinking" => AgentActivityStatus::Thinking,
            "working" => AgentActivityStatus::Working,
            "waiting_for_input" => AgentActivityStatus::WaitingForInput,
            "completed" => AgentActivityStatus::Completed,
            "error" => AgentActivityStatus::Error,
            "cancelled" => AgentActivityStatus::Cancelled,
            "disconnected" => AgentActivityStatus::Disconnected,
            _ => AgentActivityStatus::Idle,
        }
    }
}

/// Notification kinds tracked per-pane for cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotifyKind {
    Started,
    Finished,
    NeedsAttention,
    Error,
    Cancelled,
}

/// Per-type notification toggle, persisted by the frontend under the KV key
/// `"agent_notify_config"` and applied by the heartbeat each tick. All
/// defaults ON — the user opts out per type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentNotifyConfig {
    /// Working → Completed transition notification.
    pub finished: bool,
    /// Waiting-for-input / attention transition notification.
    pub needs_attention: bool,
    /// Error transition notification.
    pub error: bool,
}

impl Default for AgentNotifyConfig {
    fn default() -> Self {
        Self {
            finished: true,
            needs_attention: true,
            error: true,
        }
    }
}

impl AgentNotifyConfig {
    /// True when `kind` is enabled by this config.
    pub fn enabled(&self, kind: NotifyKind) -> bool {
        match kind {
            NotifyKind::Started | NotifyKind::Cancelled => true,
            NotifyKind::Finished => self.finished,
            NotifyKind::NeedsAttention => self.needs_attention,
            NotifyKind::Error => self.error,
        }
    }
}

/// Window (ms) for the bursty-agent promotion: when an agent is FIRST
/// detected, output within this window counts as "sustained" and promotes
/// straight to Working. Deliberately small (~3× the 1.5s heartbeat interval)
/// so a just-started agent that emitted during startup is caught, without
/// attributing stale output from a *previous* command (up to 30s old under
/// `idle_after_ms`) to the newly-detected agent.
pub const FRESH_DETECTION_OUTPUT_WINDOW_MS: u64 = 5_000;

/// Default silence threshold before an agent badge moves to idle.
pub const DEFAULT_IDLE_AFTER_MS: u64 = 30_000;
/// Minimum working duration before a "finished" notification may fire.
pub const DEFAULT_MIN_WORK_MS: u64 = 15_000;
/// Per-pane cooldown between notifications of the same kind.
pub const DEFAULT_NOTIFY_COOLDOWN_MS: u64 = 15_000;

#[derive(Debug, Clone)]
struct PaneActivity {
    pane_id: String,
    status: AgentActivityStatus,
    agent_key: Option<String>,
    /// Raw classified foreground label (`"vim"`, `"claude"`, `"node"`, …;
    /// `None` when the foreground is a shell). Mirrors what
    /// `pty_agent_info` used to return, so the frontend can drop its own
    /// `ps` polling entirely and trust `agent:status`.
    raw_fg: Option<String>,
    session_id: Option<String>,
    /// Last scraped task title / raw prompt (piggybacked on `agent:status` so
    /// the frontend can summarize without re-polling `ps`).
    task_title: Option<String>,
    raw_prompt: Option<String>,
    last_output_at: u64,
    work_started_at: Option<u64>,
    plugin_connected: bool,
    last_notified_at: HashMap<NotifyKind, u64>,
    /// Signature of the last emitted `agent:status` payload so we only emit
    /// on change.
    last_emitted: Option<(
        AgentActivityStatus,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
}

/// Public snapshot of a pane's activity, used by tests and diagnostics.
#[derive(Debug, Clone)]
pub struct PaneActivitySnapshot {
    pub pane_id: String,
    pub status: AgentActivityStatus,
    pub agent_key: Option<String>,
    pub session_id: Option<String>,
    pub work_started_at: Option<u64>,
}

/// The tracker. Internally synchronized; clone cheap (Arc-backed).
pub struct AgentActivityTracker {
    /// Serializes lifecycle operations so the retired check and heartbeat
    /// insertion are atomic with respect to register/remove.
    lifecycle: Mutex<()>,
    panes: Mutex<HashMap<String, PaneActivity>>,
    /// Pane ids whose PTY lifecycle has ended. Heartbeats can race the
    /// session-manager removal by one tick, so they must not recreate a
    /// retired pane until a new PTY explicitly registers it.
    retired_panes: Mutex<HashSet<String>>,
    /// Registration tokens used only to make PTY cleanup generation-aware.
    /// Entries are removed when their matching PTY retires, so this stays
    /// bounded by currently-live/recently-reused pane ids.
    generations: Mutex<HashMap<String, u64>>,
    next_generation: Mutex<u64>,
    event_emitter: EventEmitter,
    notifications: Option<Arc<NotificationService>>,
    /// Per-type notification toggles (read by `notify_locked`, updated by the
    /// heartbeat from the KV store).
    notify_config: Mutex<AgentNotifyConfig>,
    pub idle_after_ms: u64,
    pub min_work_ms: u64,
    pub notify_cooldown_ms: u64,
}

impl std::fmt::Debug for AgentActivityTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentActivityTracker")
            .field("panes", &"<Mutex<HashMap>>")
            .field("event_emitter", &"<EventEmitter>")
            .finish()
    }
}

impl Clone for AgentActivityTracker {
    fn clone(&self) -> Self {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        let panes = self.panes.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let retired_panes = self
            .retired_panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let generations = self
            .generations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let next_generation = *self
            .next_generation
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        Self {
            lifecycle: Mutex::new(()),
            panes: Mutex::new(panes),
            retired_panes: Mutex::new(retired_panes),
            generations: Mutex::new(generations),
            next_generation: Mutex::new(next_generation),
            event_emitter: Arc::clone(&self.event_emitter),
            notifications: self.notifications.clone(),
            notify_config: Mutex::new(
                *self.notify_config.lock().unwrap_or_else(|p| p.into_inner()),
            ),
            idle_after_ms: self.idle_after_ms,
            min_work_ms: self.min_work_ms,
            notify_cooldown_ms: self.notify_cooldown_ms,
        }
    }
}

impl AgentActivityTracker {
    pub fn new(notifications: Option<Arc<NotificationService>>) -> Self {
        Self {
            lifecycle: Mutex::new(()),
            panes: Mutex::new(HashMap::new()),
            retired_panes: Mutex::new(HashSet::new()),
            generations: Mutex::new(HashMap::new()),
            next_generation: Mutex::new(0),
            event_emitter: EventEmitter::default(),
            notifications,
            notify_config: Mutex::new(AgentNotifyConfig::default()),
            idle_after_ms: DEFAULT_IDLE_AFTER_MS,
            min_work_ms: DEFAULT_MIN_WORK_MS,
            notify_cooldown_ms: DEFAULT_NOTIFY_COOLDOWN_MS,
        }
    }

    /// Current per-type notification config.
    pub fn notify_config(&self) -> AgentNotifyConfig {
        *self.notify_config.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Replace the per-type notification config.
    pub fn set_notify_config(&self, cfg: AgentNotifyConfig) {
        *self.notify_config.lock().unwrap_or_else(|p| p.into_inner()) = cfg;
    }

    /// Set the event emitter for `agent:status` payloads.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    fn emit(&self, channel: &str, data: &serde_json::Value) {
        if let Ok(guard) = self.event_emitter.lock() {
            if let Some(ref emitter) = *guard {
                emitter(channel, data);
                return;
            }
        }
        log::debug!("[agent-activity] {} -> {}", channel, data);
    }

    fn entry_or_insert(&self, pane_id: &str) -> PaneActivity {
        let mut guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .entry(pane_id.to_string())
            .or_insert_with(|| PaneActivity {
                pane_id: pane_id.to_string(),
                status: AgentActivityStatus::Idle,
                agent_key: None,
                raw_fg: None,
                session_id: None,
                task_title: None,
                raw_prompt: None,
                last_output_at: 0,
                work_started_at: None,
                plugin_connected: false,
                last_notified_at: HashMap::new(),
                last_emitted: None,
            })
            .clone()
    }

    fn update<F>(&self, pane_id: &str, f: F)
    where
        F: FnOnce(&mut PaneActivity),
    {
        let mut guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = guard.get_mut(pane_id) {
            f(entry);
        }
    }

    fn generation_is_current(&self, pane_id: &str, generation: Option<u64>) -> bool {
        match generation {
            Some(expected) => {
                self.generations
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(pane_id)
                    .copied()
                    == Some(expected)
            }
            None => true,
        }
    }

    // -- Lifecycle ----------------------------------------------------------

    /// Ensure a pane is tracked (idempotent). Registration also marks a pane
    /// as live again, allowing a later PTY to reuse the same pane id safely.
    pub fn register_pane(&self, pane_id: &str) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        self.retired_panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(pane_id);
        self.entry_or_insert(pane_id);
    }

    /// Register a pane and return a token for the owning PTY read loop.
    /// The public `register_pane` API remains unchanged for non-PTY callers.
    pub fn register_pane_with_generation(&self, pane_id: &str) -> u64 {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        self.retired_panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(pane_id);
        let generation = {
            let mut next = self
                .next_generation
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            *next = next.saturating_add(1);
            *next
        };
        self.generations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(pane_id.to_string(), generation);
        self.entry_or_insert(pane_id);
        generation
    }

    /// Drop a pane (PTY closed). No event is emitted here — the frontend
    /// already transitions on `terminal:exit`.
    ///
    /// The retired marker prevents a heartbeat that raced the PTY teardown
    /// from recreating the entry on its next tick.
    pub fn remove_pane(&self, pane_id: &str) {
        self.remove_pane_if_generation(pane_id, None);
    }

    /// Remove a pane only if the PTY registration token is still current.
    /// An old read loop cannot retire state belonging to a newer registration
    /// that reused the same pane id.
    pub fn remove_pane_if_generation(&self, pane_id: &str, generation: Option<u64>) -> bool {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(expected) = generation {
            let current = self
                .generations
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(pane_id)
                .copied();
            if current != Some(expected) {
                return false;
            }
        }
        self.generations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(pane_id);
        self.retired_panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(pane_id.to_string());
        self.panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(pane_id);
        true
    }

    /// Mark a pane as plugin-connected (status driven by the plugin, not the
    /// tracker's heuristics).
    pub fn set_plugin_connected(&self, pane_id: &str, connected: bool) {
        self.update(pane_id, |e| e.plugin_connected = connected);
    }

    // -- Push inputs --------------------------------------------------------

    /// Record a status pushed by the plugin path (no emission — the frontend
    /// receives `agents:statusUpdate` directly with `paneId` via the adapter).
    pub fn on_plugin_status(&self, pane_id: &str, status: AgentActivityStatus) {
        self.update(pane_id, |e| {
            e.status = status;
            e.plugin_connected = true;
        });
    }

    /// PTY output arrived. An active agent goes Idle → Thinking on its first
    /// output pulse, then Thinking → Working once output is sustained, and
    /// WaitingForInput → Working when the user answers. The Thinking state
    /// renders as a pulsing dot (frontend) and counts toward the space's
    /// working badge, so a freshly-started agent reads as "waking up" rather
    /// than instantly "working".
    pub fn on_pty_output(&self, pane_id: &str, now_ms: u64) {
        self.on_pty_output_for_generation(pane_id, now_ms, None);
    }

    /// Generation-aware variant used by PTY read loops. Stale duplicate loops
    /// must not update the state belonging to a newer loop for the same pane.
    pub fn on_pty_output_for_generation(
        &self,
        pane_id: &str,
        now_ms: u64,
        generation: Option<u64>,
    ) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if !self.generation_is_current(pane_id, generation) {
            return;
        }
        let mut changed = false;
        self.update(pane_id, |e| {
            e.last_output_at = now_ms;
            if e.plugin_connected || e.agent_key.is_none() {
                return;
            }
            match e.status {
                // Fresh detection / new turn, first output pulse → thinking.
                AgentActivityStatus::Idle | AgentActivityStatus::Completed => {
                    e.status = AgentActivityStatus::Thinking;
                    changed = true;
                }
                // Sustained output → actively working (start the work clock).
                AgentActivityStatus::Thinking => {
                    e.status = AgentActivityStatus::Working;
                    if e.work_started_at.is_none() {
                        e.work_started_at = Some(now_ms);
                    }
                    changed = true;
                }
                // User answered a prompt → back to working.
                AgentActivityStatus::WaitingForInput => {
                    e.status = AgentActivityStatus::Working;
                    if e.work_started_at.is_none() {
                        e.work_started_at = Some(now_ms);
                    }
                    changed = true;
                }
                _ => {}
            }
        });
        drop(_lifecycle);
        if changed {
            self.emit_status(pane_id);
        }
    }

    /// The pane's shell reported its foreground command finished. When that
    /// command was the agent's launch command, the agent process has exited —
    /// a positive completion signal.
    pub fn on_shell_command_finished(&self, pane_id: &str, command: &str, now_ms: u64) {
        self.on_shell_command_finished_with_exit_code(pane_id, command, 0, now_ms);
    }

    /// Handle an agent launch command completing, including its exit code.
    /// A non-zero exit is an explicit failure and is surfaced immediately;
    /// successful completion still observes the minimum-work gate to avoid
    /// turning a short-lived startup probe into a completion alert.
    pub fn on_shell_command_finished_with_exit_code(
        &self,
        pane_id: &str,
        command: &str,
        exit_code: i32,
        now_ms: u64,
    ) {
        self.on_shell_command_finished_with_exit_code_for_generation(
            pane_id, command, exit_code, now_ms, None,
        );
    }

    /// Generation-aware variant used by PTY read loops.
    pub fn on_shell_command_finished_for_generation(
        &self,
        pane_id: &str,
        command: &str,
        now_ms: u64,
        generation: Option<u64>,
    ) {
        self.on_shell_command_finished_with_exit_code_for_generation(
            pane_id, command, 0, now_ms, generation,
        );
    }

    /// Generation-aware completion handler with an explicit process exit code.
    pub fn on_shell_command_finished_with_exit_code_for_generation(
        &self,
        pane_id: &str,
        command: &str,
        exit_code: i32,
        now_ms: u64,
        generation: Option<u64>,
    ) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if !self.generation_is_current(pane_id, generation) {
            return;
        }
        let mut changed = false;
        self.update(pane_id, |e| {
            if e.plugin_connected || e.agent_key.is_none() {
                return;
            }
            // Only treat as completion when the finished command is (a start
            // of) the agent's own launch command. Background `ls` while an
            // agent runs must never fire "finished".
            let is_agent_launch = e
                .agent_key
                .as_deref()
                .map(|key| {
                    let c = command.trim();
                    !c.is_empty()
                        && c.split_whitespace().any(|w| {
                            let base = w.split(['/', '=']).last().unwrap_or(w);
                            base == key
                                || base.contains(key)
                                || key == "omp" && (base == "omp" || base == "oh-my-pi")
                        })
                })
                .unwrap_or(false);
            if !is_agent_launch {
                return;
            }
            let work_ok = e
                .work_started_at
                .map(|start| now_ms.saturating_sub(start) >= self.min_work_ms)
                .unwrap_or(false);
            if exit_code != 0 {
                self.notify_locked(e, NotifyKind::Error, now_ms);
                e.status = AgentActivityStatus::Error;
            } else {
                if e.status == AgentActivityStatus::Working && work_ok {
                    self.notify_locked(e, NotifyKind::Finished, now_ms);
                }
                e.status = AgentActivityStatus::Completed;
            }
            e.agent_key = None;
            e.work_started_at = None;
            changed = true;
        });
        drop(_lifecycle);
        if changed {
            self.emit_status(pane_id);
        }
    }

    // -- Heartbeat (slow poll) ----------------------------------------------

    /// Heartbeat from the poll loop: foreground-process label, session-file
    /// snapshot, and the stripped tail of recent output.
    ///
    /// `fg_label` is the [`crate::agent_detection::classify_foreground_ps`]
    /// result for the pane's controlling terminal (`"shell"` when no agent).
    pub fn heartbeat(
        &self,
        pane_id: &str,
        fg_label: Option<&str>,
        history: Option<&HistorySnapshot>,
        output_tail: Option<&str>,
        now_ms: u64,
    ) {
        // Hold the lifecycle lock across the retired check and all state
        // mutation so a remove cannot slip between them and allow stale state
        // to reappear or update a newly reused pane.
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if self
            .retired_panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(pane_id)
        {
            return;
        }

        // Ensure the pane is tracked (idempotent registration).
        self.entry_or_insert(pane_id);
        let mut changed = false;

        {
            let mut guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
            let Some(e) = guard.get_mut(pane_id) else {
                return;
            };

            if e.plugin_connected {
                // Plugin drives status (and the state.rs adapter re-emits
                // `agents:*` with paneId); the tracker never emits for these
                // panes, so nothing to track here.
                return;
            }

            // Track the raw foreground label on every heartbeat so the
            // payload mirrors `pty_agent_info` (the frontend no longer polls
            // `ps` itself — `agent:status` is its single source of truth).
            // A shell foreground is `None` (no "shell" fgProcess badge).
            if e.raw_fg.as_deref() != fg_label {
                e.raw_fg = fg_label.map(|s| s.to_string());
                changed = true;
            }

            let agent_key: Option<&str> =
                fg_label.filter(|f| crate::agent_detection::is_known_agent_key(f));

            match agent_key {
                Some(key) => {
                    // First detection (or re-detection after completion).
                    if e.agent_key.as_deref() != Some(key) {
                        e.agent_key = Some(key.to_string());
                        // If the agent already emitted output before the first
                        // heartbeat saw it (output pulses are dropped while
                        // agent_key is None), promote straight to Working — a
                        // bursty agent (echo one; echo two; then a long silent
                        // tool call) would otherwise sit Idle for its whole
                        // run and only show Working at exit. Output within the
                        // fresh-detection window counts as "sustained" (kept
                        // well below idle_after_ms so stale output from a
                        // previous command isn't attributed to the new agent).
                        let has_fresh_output = e.last_output_at > 0
                            && now_ms.saturating_sub(e.last_output_at)
                                <= FRESH_DETECTION_OUTPUT_WINDOW_MS;
                        if has_fresh_output {
                            e.status = AgentActivityStatus::Working;
                            e.work_started_at = Some(now_ms);
                        } else {
                            e.status = AgentActivityStatus::Idle;
                            e.work_started_at = None;
                        }
                        changed = true;
                    }
                    // New turn started (session file advanced).
                    if let Some(h) = history {
                        if e.session_id.as_deref() != Some(h.session_id.as_str()) {
                            e.session_id = Some(h.session_id.clone());
                            e.task_title = Some(h.task_title.clone());
                            e.raw_prompt = Some(h.raw_prompt.clone());
                            if e.status == AgentActivityStatus::Completed {
                                e.status = AgentActivityStatus::Idle;
                                e.work_started_at = None;
                                changed = true;
                            }
                        }
                    }
                    // Silence → badge idle only (never a notification). Applies
                    // to both Working and Thinking (a one-pulse "thinking"
                    // agent that goes quiet must dim back too).
                    //
                    // NOTE: `work_started_at` deliberately SURVIVES silence.
                    // An agent can do 10s of output then run a long silent
                    // tool call (build/upload) and exit with no further
                    // output; if silence cleared the clock, the "finished"
                    // notification (heartbeat None-branch) would be suppressed
                    // for a genuinely completed agent. The min_work gate + 15s
                    // cooldown already prevent spam, so keeping the clock
                    // across silence is safe.
                    if matches!(
                        e.status,
                        AgentActivityStatus::Working | AgentActivityStatus::Thinking
                    ) && e.last_output_at > 0
                        && now_ms.saturating_sub(e.last_output_at) > self.idle_after_ms
                    {
                        e.status = AgentActivityStatus::Idle;
                        changed = true;
                    }
                    // Waiting-for-input heuristic (native agents only).
                    if e.status != AgentActivityStatus::WaitingForInput
                        && output_tail
                            .map(tail_looks_waiting_for_input)
                            .unwrap_or(false)
                    {
                        e.status = AgentActivityStatus::WaitingForInput;
                        changed = true;
                        self.notify_locked(e, NotifyKind::NeedsAttention, now_ms);
                    }
                }
                None => {
                    // Foreground returned to the shell while an agent was
                    // working → positive completion signal.
                    if e.agent_key.is_some() {
                        let work_ok = e
                            .work_started_at
                            .map(|start| now_ms.saturating_sub(start) >= self.min_work_ms)
                            .unwrap_or(false);
                        if e.status == AgentActivityStatus::Working && work_ok {
                            self.notify_locked(e, NotifyKind::Finished, now_ms);
                        }
                        e.status = AgentActivityStatus::Completed;
                        e.agent_key = None;
                        e.work_started_at = None;
                        changed = true;
                    }
                }
            }
        }

        if changed {
            self.emit_status(pane_id);
        }
    }

    // -- Notification + emission --------------------------------------------

    fn notify_locked(&self, e: &mut PaneActivity, kind: NotifyKind, now_ms: u64) {
        // Per-type toggle (frontend setting) gates the notification.
        if !self.notify_config().enabled(kind) {
            return;
        }
        let cooldown_ok = e
            .last_notified_at
            .get(&kind)
            .map(|last| now_ms.saturating_sub(*last) > self.notify_cooldown_ms)
            .unwrap_or(true);
        if !cooldown_ok {
            return;
        }
        // Record the cooldown before pushing so a failing push cannot cause
        // a retry storm on the next heartbeat.
        e.last_notified_at.insert(kind, now_ms);
        let Some(ref svc) = self.notifications else {
            return;
        };
        let label = e
            .agent_key
            .as_deref()
            .and_then(agent_label)
            .unwrap_or("Agent");
        let (ntype, title, message) = match kind {
            NotifyKind::Started => (
                NotificationType::Info,
                "Agent started",
                format!("{label} started working in pane {}", e.pane_id),
            ),
            NotifyKind::Finished => (
                NotificationType::Success,
                "Agent finished",
                format!("{label} finished its work in pane {}", e.pane_id),
            ),
            NotifyKind::NeedsAttention => (
                NotificationType::NeedsInput,
                "Agent needs attention",
                format!("{label} in pane {} is waiting for input", e.pane_id),
            ),
            NotifyKind::Error => (
                NotificationType::TaskError,
                "Agent error",
                format!("{label} in pane {} hit an error", e.pane_id),
            ),
            NotifyKind::Cancelled => (
                NotificationType::Warning,
                "Agent cancelled",
                format!("{label} in pane {} was cancelled", e.pane_id),
            ),
        };
        let event = NotificationEvent {
            r#type: ntype,
            title: title.to_string(),
            message,
            source: "agent".to_string(),
            agent_id: Some(e.pane_id.clone()),
            data: Some(serde_json::json!({ "paneId": e.pane_id })),
            timestamp: now_ms,
            metadata: None,
            actions: None,
            request_id: None,
        };
        svc.push_notification(event);
    }

    fn emit_status(&self, pane_id: &str) {
        let snapshot = {
            let guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
            guard.get(pane_id).cloned()
        };
        let Some(e) = snapshot else { return };

        let signature = (
            e.status,
            e.agent_key.clone(),
            e.raw_fg.clone(),
            e.session_id.clone(),
            e.task_title.clone(),
            e.raw_prompt.clone(),
        );
        if e.last_emitted.as_ref() == Some(&signature) {
            return;
        }
        self.update(pane_id, |entry| entry.last_emitted = Some(signature));

        let payload = serde_json::json!({
            "paneId": e.pane_id,
            "status": e.status.as_str(),
            "message": status_message(&e.status),
            // Raw foreground label (any process, not just known agents) so
            // the frontend keeps its pill badge + smart titles in sync
            // without a `ps` poll of its own.
            "fgProcess": e.raw_fg.clone().unwrap_or_default(),
            "sessionId": e.session_id.clone().unwrap_or_default(),
            "taskTitle": e.task_title.clone().unwrap_or_default(),
            "rawPrompt": e.raw_prompt.clone().unwrap_or_default(),
            "timestamp": now_ms(),
        });
        self.emit("agent:status", &payload);
    }

    /// Record an agent launch before its first heartbeat. This gives explicit
    /// spawn paths a durable "started" event and primes the tracker so the
    /// later foreground heartbeat does not duplicate it.
    pub fn notify_agent_started(&self, pane_id: &str, agent_key: &str, now_ms: u64) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        self.entry_or_insert(pane_id);
        self.update(pane_id, |e| {
            if e.agent_key.is_none() {
                e.agent_key = Some(agent_key.to_string());
                self.notify_locked(e, NotifyKind::Started, now_ms);
            }
        });
    }

    /// Record an explicit user/tool cancellation before the PTY is killed.
    /// This is separate from natural PTY exit so cancellation is not confused
    /// with a disconnected terminal and the user receives a durable alert.
    pub fn cancel_pane(&self, pane_id: &str, now_ms: u64) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        let mut changed = false;
        self.update(pane_id, |e| {
            if e.agent_key.is_some()
                && !matches!(
                    e.status,
                    AgentActivityStatus::Completed
                        | AgentActivityStatus::Error
                        | AgentActivityStatus::Cancelled
                        | AgentActivityStatus::Disconnected
                )
            {
                self.notify_locked(e, NotifyKind::Cancelled, now_ms);
                e.status = AgentActivityStatus::Cancelled;
                e.agent_key = None;
                e.work_started_at = None;
                changed = true;
            }
        });
        drop(_lifecycle);
        if changed {
            self.emit_status(pane_id);
        }
    }

    /// Snapshot of all tracked panes (tests + diagnostics).
    pub fn snapshot(&self) -> Vec<PaneActivitySnapshot> {
        let guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .values()
            .map(|e| PaneActivitySnapshot {
                pane_id: e.pane_id.clone(),
                status: e.status,
                agent_key: e.agent_key.clone(),
                session_id: e.session_id.clone(),
                work_started_at: e.work_started_at,
            })
            .collect()
    }

    /// Registered pane ids (heartbeat driver iterates these).
    pub fn pane_ids(&self) -> Vec<String> {
        self.panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .cloned()
            .collect()
    }
}

fn status_message(status: &AgentActivityStatus) -> &'static str {
    match status {
        AgentActivityStatus::Working => "working",
        AgentActivityStatus::WaitingForInput => "waiting for input",
        AgentActivityStatus::Completed => "completed",
        AgentActivityStatus::Error => "error",
        AgentActivityStatus::Thinking => "thinking",
        AgentActivityStatus::Disconnected => "disconnected",
        AgentActivityStatus::Cancelled => "cancelled",
        AgentActivityStatus::Idle => "idle",
    }
}

/// Conservative matcher: the stripped output tail contains a permission /
/// confirmation prompt marker in its final lines. Table-driven; best-effort.
pub fn tail_looks_waiting_for_input(tail: &str) -> bool {
    const MARKERS: &[&str] = &[
        "allow?",
        "(y/n)",
        "y/n?",
        "[y/n]",
        "yes/no",
        "(yes/no)",
        "press enter to",
        "proceed?",
        "do you want to continue",
    ];
    let lines: Vec<&str> = tail.lines().collect();
    let check_last = lines.iter().rev().take(8);
    for line in check_last {
        let lower = line.to_lowercase();
        if MARKERS.iter().any(|m| lower.contains(m)) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::NotificationService;
    use std::sync::Mutex as StdMutex;

    fn tracker() -> (AgentActivityTracker, Arc<StdMutex<Vec<serde_json::Value>>>) {
        let mut t = AgentActivityTracker::new(Some(Arc::new(NotificationService::new())));
        t.idle_after_ms = 100;
        t.min_work_ms = 50;
        t.notify_cooldown_ms = 50;
        let events = Arc::new(StdMutex::new(Vec::new()));
        let ev = events.clone();
        t.set_event_emitter(move |_channel, data| ev.lock().unwrap().push(data.clone()));
        (t, events)
    }

    fn status_of(t: &AgentActivityTracker, pane: &str) -> AgentActivityStatus {
        t.snapshot()
            .into_iter()
            .find(|s| s.pane_id == pane)
            .map(|s| s.status)
            .unwrap_or(AgentActivityStatus::Idle)
    }

    #[test]
    fn detects_agent_and_emits_working() {
        let (t, events) = tracker();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        // First output pulse → thinking; sustained output → working.
        t.on_pty_output("p1", 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Thinking);
        t.on_pty_output("p1", 1201);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
        let emitted: Vec<String> = events
            .lock()
            .unwrap()
            .iter()
            .map(|v| {
                v.get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert!(emitted.contains(&"working".to_string()));
        assert!(emitted.contains(&"thinking".to_string()));
    }

    #[test]
    fn bursty_agent_with_pre_detection_output_promotes_straight_to_working() {
        // Output pulses arriving BEFORE the first heartbeat that registers the
        // agent (agent_key is still None) used to be dropped by
        // `on_pty_output`; the agent then sat Idle for its whole run and only
        // showed Working at exit. The heartbeat must promote a freshly
        // detected agent straight to Working when output arrived recently
        // (within idle_after_ms).
        let (mut t, _events) = tracker();
        // The pane is already tracked (the production heartbeat loop inserts
        // every live session on its first tick). Widen the silence threshold so
        // the fresh-detection window (not the test's 100ms idle_after_ms) is
        // the binding constraint — in production idle_after_ms is 30s.
        t.register_pane("p1");
        t.idle_after_ms = 10_000;
        // Output pulses arrive while no agent is detected yet.
        t.on_pty_output("p1", 1000);
        t.on_pty_output("p1", 1001);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        // First detection: recent output (within
        // FRESH_DETECTION_OUTPUT_WINDOW_MS) → straight to Working.
        t.heartbeat("p1", Some("claude"), None, None, 3000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
    }

    #[test]
    fn freshly_detected_agent_with_no_recent_output_stays_idle() {
        let (t, _events) = tracker();
        // Output long ago (beyond FRESH_DETECTION_OUTPUT_WINDOW_MS) →
        // detection stays Idle; the next pulse will promote to Thinking →
        // Working.
        t.on_pty_output("p1", 1000);
        t.heartbeat("p1", Some("claude"), None, None, 7000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
    }

    #[test]
    fn one_pulse_stays_thinking_until_sustained() {
        let (t, _events) = tracker();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Thinking);
        // A single pulse then silence → dims back to idle (no "working"
        // badge, no notification).
        t.heartbeat("p1", Some("claude"), None, None, 1500);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
    }

    #[test]
    fn waiting_input_returns_to_working_on_answer() {
        let (t, _events) = tracker();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1200);
        t.on_pty_output("p1", 1201);
        t.heartbeat("p1", Some("claude"), None, Some("(y/n)"), 2000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
        // User answers → output resumes → working again.
        t.on_pty_output("p1", 2100);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
    }

    #[test]
    fn completion_requires_positive_signal_not_silence() {
        let (t, events) = tracker();
        let svc_arc = t.notifications.clone().unwrap();

        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1200);
        t.on_pty_output("p1", 1201);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);

        // Silence alone → idle badge, NO "finished" notification.
        t.heartbeat("p1", Some("claude"), None, Some("some output here"), 1500);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        assert_eq!(svc_arc.get_history(None).len(), 0);

        // Working again, then the agent process exits to the shell → finished.
        t.on_pty_output("p1", 1600);
        t.on_pty_output("p1", 1601);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
        t.heartbeat("p1", None, None, None, 2000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        let history = svc_arc.get_history(None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].title, "Agent finished");
        let _ = events;
    }

    #[test]
    fn completion_respects_min_work_gate() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        // Very short work (< min_work_ms): no notification.
        t.heartbeat("p1", Some("codex"), None, None, 1000);
        t.on_pty_output("p1", 1005);
        t.on_pty_output("p1", 1006);
        t.heartbeat("p1", None, None, None, 1010);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        assert_eq!(svc.get_history(None).len(), 0);
    }

    #[test]
    fn waiting_for_input_triggers_needs_attention() {
        let (t, events) = tracker();
        let svc = t.notifications.clone().unwrap();

        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.heartbeat(
            "p1",
            Some("claude"),
            None,
            Some("Running tool...\nDo you want to continue? (y/n)"),
            2000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
        let history = svc.get_history(None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].r#type, NotificationType::NeedsInput);
        let _ = events;
    }

    #[test]
    fn notification_cooldown_prevents_spam() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.heartbeat("p1", None, None, None, 1200); // finished #1
        assert_eq!(svc.get_history(None).len(), 1);

        // New turn → working → finished again within cooldown → no notify.
        t.heartbeat("p1", Some("claude"), None, None, 1220);
        t.on_pty_output("p1", 1230);
        t.on_pty_output("p1", 1231);
        t.heartbeat("p1", None, None, None, 1240);
        assert_eq!(svc.get_history(None).len(), 1);

        // After cooldown elapses, a third finish notifies again (with a
        // work window that clears the min_work gate).
        t.heartbeat("p1", Some("claude"), None, None, 1400);
        t.on_pty_output("p1", 1410);
        t.on_pty_output("p1", 1411);
        t.heartbeat("p1", None, None, None, 1470);
        assert_eq!(svc.get_history(None).len(), 2);
    }

    #[test]
    fn notify_config_gates_notifications() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();

        // Finished notifications off → no history entries.
        t.set_notify_config(AgentNotifyConfig {
            finished: false,
            needs_attention: true,
            error: true,
        });
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.heartbeat("p1", None, None, None, 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        assert_eq!(svc.get_history(None).len(), 0);

        // Re-enabled → a fresh finish notifies again.
        t.set_notify_config(AgentNotifyConfig {
            finished: true,
            needs_attention: true,
            error: true,
        });
        t.heartbeat("p1", Some("claude"), None, None, 1400);
        t.on_pty_output("p1", 1500);
        t.on_pty_output("p1", 1501);
        t.heartbeat("p1", None, None, None, 1600);
        assert_eq!(svc.get_history(None).len(), 1);
    }

    #[test]
    fn notify_config_gates_needs_attention() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.set_notify_config(AgentNotifyConfig {
            finished: true,
            needs_attention: false,
            error: true,
        });
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.heartbeat(
            "p1",
            Some("claude"),
            None,
            Some("Do you want to continue? (y/n)"),
            2000,
        );
        // Status still transitions (badge), but no notification fires.
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
        assert_eq!(svc.get_history(None).len(), 0);
    }

    #[test]
    fn notify_config_defaults_all_on() {
        let cfg = AgentNotifyConfig::default();
        assert!(cfg.enabled(NotifyKind::Finished));
        assert!(cfg.enabled(NotifyKind::NeedsAttention));
        assert!(cfg.enabled(NotifyKind::Error));
    }

    #[test]
    fn plugin_connected_skips_heuristics() {
        let (t, events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.set_plugin_connected("p1", true);
        // Even with a waiting-input-looking tail and no foreground, no change.
        t.heartbeat("p1", None, None, Some("(y/n)"), 2000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        assert_eq!(svc.get_history(None).len(), 0);
        let _ = events;
    }
    #[test]
    fn shell_command_finished_detects_agent_exit() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        // The shell reports `claude` finished → completion.
        t.on_shell_command_finished("p1", "claude --dangerously-skip-permissions", 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        assert_eq!(svc.get_history(None).len(), 1);
        assert_eq!(svc.get_history(None)[0].title, "Agent finished");
    }

    #[test]
    fn nonzero_shell_command_exit_notifies_error() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.heartbeat("p1", Some("codex"), None, None, 1000);
        t.on_pty_output("p1", 1001);
        t.on_pty_output("p1", 1002);
        t.on_shell_command_finished_with_exit_code("p1", "codex", 1, 1010);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Error);
        assert_eq!(svc.get_history(None).len(), 1);
        assert_eq!(svc.get_history(None)[0].r#type, NotificationType::TaskError);
    }

    #[test]
    fn empty_shell_command_does_not_complete_agent() {
        let (t, _events) = tracker();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.on_shell_command_finished("p1", "", 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
    }

    #[test]
    fn shell_command_finished_ignores_unrelated_commands() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.on_shell_command_finished("p1", "ls -la", 1200);
        // Not the agent launch → still working.
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
        assert_eq!(svc.get_history(None).len(), 0);
    }

    #[test]
    fn session_id_change_resets_completed_turn() {
        let (t, _events) = tracker();
        let h1 = HistorySnapshot {
            task_title: "task one".into(),
            session_id: "s1".into(),
            timestamp_ms: 1,
            raw_prompt: "task one".into(),
        };
        let h2 = HistorySnapshot {
            task_title: "task two".into(),
            session_id: "s2".into(),
            timestamp_ms: 2,
            raw_prompt: "task two".into(),
        };
        t.heartbeat("p1", Some("claude"), Some(&h1), None, 1000);
        t.on_pty_output("p1", 1100);
        t.heartbeat("p1", None, None, None, 1200); // completed
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        // New session id → reset to idle, ready for the next turn.
        t.heartbeat("p1", Some("claude"), Some(&h2), None, 1300);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
    }

    #[test]
    fn tail_matcher_is_conservative() {
        assert!(tail_looks_waiting_for_input(
            "Do you want to continue? (y/n)"
        ));
        assert!(tail_looks_waiting_for_input("Allow?"));
        assert!(tail_looks_waiting_for_input("Proceed? [y/n]"));
        assert!(!tail_looks_waiting_for_input(
            "compiling project (yes, this is normal)"
        ));
        assert!(!tail_looks_waiting_for_input(""));
        assert!(!tail_looks_waiting_for_input("nothing interesting here"));
    }

    #[test]
    fn explicit_agent_start_notifies_once() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.notify_agent_started("p1", "claude", 1000);
        t.notify_agent_started("p1", "claude", 1001);
        assert_eq!(svc.get_history(None).len(), 1);
        assert_eq!(svc.get_history(None)[0].title, "Agent started");
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
    }

    #[test]
    fn explicit_cancellation_notifies_active_agent() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.notify_agent_started("p1", "codex", 1000);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.cancel_pane("p1", 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Cancelled);
        let history = svc.get_history(None);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].title, "Agent cancelled");
    }

    #[test]
    fn status_round_trips() {
        for s in [
            AgentActivityStatus::Idle,
            AgentActivityStatus::Thinking,
            AgentActivityStatus::Working,
            AgentActivityStatus::WaitingForInput,
            AgentActivityStatus::Completed,
            AgentActivityStatus::Error,
            AgentActivityStatus::Cancelled,
            AgentActivityStatus::Disconnected,
        ] {
            assert_eq!(AgentActivityStatus::from_str(s.as_str()), s);
        }
    }

    #[test]
    fn remove_pane_cleans_up_tracked_state() {
        let (t, _events) = tracker();
        t.register_pane("p1");
        t.heartbeat("p1", Some("claude"), None, None, 1000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        assert_eq!(t.snapshot().len(), 1);

        t.remove_pane("p1");

        assert!(t.snapshot().is_empty());
        // A final heartbeat racing PTY teardown must not recreate the state.
        t.heartbeat("p1", Some("claude"), None, None, 1100);
        assert!(t.snapshot().is_empty());

        // A new PTY explicitly registering the reused pane id can start fresh.
        t.register_pane("p1");
        assert_eq!(t.snapshot().len(), 1);
    }

    #[test]
    fn stale_pty_generation_cannot_remove_reused_pane() {
        let (t, _events) = tracker();
        let old_generation = t.register_pane_with_generation("p1");
        let new_generation = t.register_pane_with_generation("p1");

        assert_ne!(old_generation, new_generation);
        assert!(!t.remove_pane_if_generation("p1", Some(old_generation)));
        assert_eq!(t.snapshot().len(), 1);

        // Make the replacement PTY actively working first. Stale
        // output/completion callbacks from the old PTY must not change it.
        t.heartbeat("p1", Some("claude"), None, None, 900);
        t.on_pty_output_for_generation("p1", 1000, Some(new_generation));
        t.on_pty_output_for_generation("p1", 1001, Some(new_generation));
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
        t.on_pty_output_for_generation("p1", 1100, Some(old_generation));
        t.on_shell_command_finished_for_generation("p1", "claude", 1101, Some(old_generation));
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);

        // A generic registration does not invalidate the PTY-owned token.
        t.register_pane("p1");
        assert!(t.remove_pane_if_generation("p1", Some(new_generation)));
        assert!(t.snapshot().is_empty());
    }
}
