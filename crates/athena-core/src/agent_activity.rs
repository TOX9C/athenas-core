//! Agent activity tracker: a backend-owned, per-pane state machine that
//! derives live agent status from PTY output, foreground-process lifecycle,
//! shell-integration events, and per-agent session files.
//!
//! Design rules (from the agent-activity notifications plan):
//! - **Plugin-status-wins**: panes with a connected plugin-host session skip
//!   all heuristics; the plugin drives their status (the state.rs adapter
//!   translates `agents:*` events to add `paneId`).
//! - **"Finished" requires a positive signal**: the agent process exiting
//!   (foreground returns to shell) or the shell reporting the agent's launch
//!   command finished. Output silence alone only moves the badge to idle.
//! - **Notifications fire on transitions only**, with per-pane cooldown.

use crate::agent_detection::{
    agent_label, command_contains_agent, AgentHistoryStatus, HistorySnapshot,
};
use crate::agent_lifecycle::{AgentLifecycleEvent, AgentLifecycleKind};
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

    /// Parse a wire-format activity status. Kept as an inherent compatibility
    /// helper while the public enum retains its original API shape.
    #[allow(clippy::should_implement_trait)]
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

impl NotifyKind {
    fn as_str(self) -> &'static str {
        match self {
            NotifyKind::Started => "started",
            NotifyKind::Finished => "finished",
            NotifyKind::NeedsAttention => "needs_attention",
            NotifyKind::Error => "error",
            NotifyKind::Cancelled => "cancelled",
        }
    }
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

/// Default silence threshold before an agent badge moves to idle.
pub const DEFAULT_IDLE_AFTER_MS: u64 = 30_000;
/// Minimum working duration before a "finished" notification may fire.
pub const DEFAULT_MIN_WORK_MS: u64 = 15_000;
/// Per-pane cooldown between notifications of the same kind.
pub const DEFAULT_NOTIFY_COOLDOWN_MS: u64 = 15_000;

type EmittedSignature = (
    AgentActivityStatus,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u64>,
);

#[derive(Debug, Clone)]
struct PaneActivity {
    pane_id: String,
    /// PTY registration generation that owns this activity record. Included
    /// in lifecycle events so a late exit from an older PTY cannot clear a
    /// newly reused pane id in the frontend.
    generation: Option<u64>,
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
    /// Timestamp of the latest history snapshot consumed for this pane. OMP
    /// keeps one session id across many prompts, so session id alone cannot
    /// identify a new turn.
    history_timestamp_ms: Option<u64>,
    last_output_at: u64,
    work_started_at: Option<u64>,
    /// Explicit agent launches print startup banners before the first
    /// heartbeat. Ignore those bytes so an idle CLI is not counted as working.
    startup_pending: bool,
    plugin_connected: bool,
    last_notified_at: HashMap<NotifyKind, u64>,
    /// Signature of the last emitted `agent:status` payload so we only emit
    /// on change.
    last_emitted: Option<EmittedSignature>,
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
        // Activity payloads may include task titles and prompt-derived metadata.
        log::debug!("[agent-activity] event emitted on channel {channel}");
    }

    fn entry_or_insert(&self, pane_id: &str) -> PaneActivity {
        let mut guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .entry(pane_id.to_string())
            .or_insert_with(|| PaneActivity {
                pane_id: pane_id.to_string(),
                generation: None,
                status: AgentActivityStatus::Idle,
                agent_key: None,
                raw_fg: None,
                session_id: None,
                task_title: None,
                raw_prompt: None,
                history_timestamp_ms: None,
                last_output_at: 0,
                work_started_at: None,
                startup_pending: false,
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
        self.update(pane_id, |entry| {
            entry.generation = Some(generation);
            entry.last_emitted = None;
        });
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

    /// Push an authoritative lifecycle transition reported by the agent itself
    /// via the OSC 6337 notification protocol (or a plugin adapter). Unlike the
    /// heartbeat heuristics, this fires immediately — no poll — and skips the
    /// min-work gate, because an explicit agent signal is always trustworthy.
    /// Generation-aware: a stale read loop must not move a newer pane.
    pub fn on_agent_lifecycle(
        &self,
        pane_id: &str,
        event: &AgentLifecycleEvent,
        generation: Option<u64>,
        now_ms: u64,
    ) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        if !self.generation_is_current(pane_id, generation) {
            return;
        }
        if self
            .retired_panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(pane_id)
        {
            return;
        }
        self.entry_or_insert(pane_id);
        let mut changed = false;
        self.update(pane_id, |e| {
            if e.plugin_connected {
                return;
            }
            // Adopt the agent key from the event when the foreground
            // classifier hasn't caught up yet, so the notification carries a
            // real label and the pane is treated as an agent going forward.
            if let Some(key) = event.agent.as_deref() {
                if crate::agent_detection::is_known_agent_key(key) && e.agent_key.is_none() {
                    e.agent_key = Some(key.to_string());
                    if e.raw_fg.is_none() {
                        e.raw_fg = Some(key.to_string());
                    }
                }
            }
            if let Some(session_id) = event.session_id.as_deref() {
                if e.session_id.as_deref() != Some(session_id) {
                    e.session_id = Some(session_id.to_string());
                }
            }
            let status = match event.kind {
                AgentLifecycleKind::Complete => AgentHistoryStatus::Completed,
                AgentLifecycleKind::Request => AgentHistoryStatus::WaitingForInput,
                AgentLifecycleKind::Error => AgentHistoryStatus::Error,
            };
            changed = self.apply_pushed_status(e, status, now_ms);
        });
        drop(_lifecycle);
        if changed {
            self.emit_status(pane_id);
        }
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
            // A freshly launched CLI emits banners, warnings, and its idle
            // prompt before the first heartbeat has confirmed the foreground
            // process. Do not turn that startup noise into a working badge.
            if e.startup_pending {
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
                .map(|key| command_contains_agent(command, key))
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
                    // Explicit launch paths mark startup output as pending.
                    // Once the heartbeat confirms the expected foreground
                    // agent, later output pulses represent real activity.
                    if e.startup_pending && e.agent_key.as_deref() == Some(key) {
                        e.startup_pending = false;
                        changed = true;
                    }
                    // First detection (or re-detection after completion).
                    if e.agent_key.as_deref() != Some(key) {
                        e.agent_key = Some(key.to_string());
                        // Detection alone is not proof of active work. Agent
                        // CLIs print startup banners, warnings, and prompts
                        // while they are idle; attributing that pre-detection
                        // output to Working makes an untouched Claude pane
                        // look active. Start at Idle and only promote after a
                        // subsequent output pulse while the agent is known.
                        e.status = AgentActivityStatus::Idle;
                        e.work_started_at = None;
                        e.history_timestamp_ms = None;
                        changed = true;
                    }
                    // New prompt / session metadata. OMP keeps a single
                    // session id across turns, so compare its timestamp and
                    // title as well as the session id.
                    if let Some(h) = history {
                        let history_changed = e.session_id.as_deref()
                            != Some(h.session_id.as_str())
                            || e.history_timestamp_ms != Some(h.timestamp_ms)
                            || e.task_title.as_deref() != Some(h.task_title.as_str());
                        if history_changed {
                            e.session_id = Some(h.session_id.clone());
                            e.task_title = Some(h.task_title.clone());
                            e.raw_prompt = Some(h.raw_prompt.clone());
                            e.history_timestamp_ms = Some(h.timestamp_ms);
                            changed = true;
                        }
                    }

                    // A harness-owned session lifecycle record is stronger
                    // than output volume or silence. This is what lets a
                    // persistent OMP process move to Completed while it stays
                    // in the foreground after returning to its input editor.
                    let authoritative_status = history.and_then(|h| h.activity);
                    if let Some(status) = authoritative_status {
                        changed |= self.apply_history_status(e, status, now_ms);
                    }

                    // Silence → badge idle only (never a notification). Do
                    // not override an authoritative "working" record: model
                    // calls and tools may be quiet for minutes.
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
                    ) && authoritative_status != Some(AgentHistoryStatus::Working)
                        && e.last_output_at > 0
                        && now_ms.saturating_sub(e.last_output_at) > self.idle_after_ms
                    {
                        e.status = AgentActivityStatus::Idle;
                        changed = true;
                    }
                    // Waiting-for-input output remains a useful fallback for
                    // native agents and for OMP confirmation prompts, but do
                    // not let stale tail text undo an authoritative completed
                    // or error state from the session log.
                    if !matches!(
                        authoritative_status,
                        Some(AgentHistoryStatus::Completed | AgentHistoryStatus::Error)
                    ) && e.status != AgentActivityStatus::WaitingForInput
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

    /// Apply a positive lifecycle state reconstructed from a harness session
    /// file. The agent key intentionally remains set for Completed/Waiting:
    /// persistent TUIs keep the same foreground process across turns.
    fn apply_history_status(
        &self,
        entry: &mut PaneActivity,
        status: AgentHistoryStatus,
        now_ms: u64,
    ) -> bool {
        // Heartbeat-path reconciliation honors the min-work gate so a
        // short-lived startup probe cannot produce a "finished" alert.
        self.apply_status(entry, status, now_ms, true)
    }

    /// Apply an authoritative status pushed by the agent itself (OSC 6337).
    /// Same transitions as [`Self::apply_history_status`], but the min-work
    /// gate is bypassed — an explicit agent signal is always trustworthy.
    fn apply_pushed_status(
        &self,
        entry: &mut PaneActivity,
        status: AgentHistoryStatus,
        now_ms: u64,
    ) -> bool {
        self.apply_status(entry, status, now_ms, false)
    }

    fn apply_status(
        &self,
        entry: &mut PaneActivity,
        status: AgentHistoryStatus,
        now_ms: u64,
        gate_min_work: bool,
    ) -> bool {
        match status {
            AgentHistoryStatus::Working => {
                // A durable log can say that the turn is still in flight while
                // the terminal is visibly paused at a permission prompt. Keep
                // the higher-fidelity WaitingForInput state until user output
                // resumes (on_pty_output transitions it back to Working).
                if matches!(
                    entry.status,
                    AgentActivityStatus::Working | AgentActivityStatus::WaitingForInput
                ) {
                    return false;
                }
                entry.status = AgentActivityStatus::Working;
                if entry.work_started_at.is_none() {
                    entry.work_started_at = Some(now_ms);
                }
                true
            }
            AgentHistoryStatus::Completed => {
                if entry.status == AgentActivityStatus::Completed {
                    return false;
                }
                if gate_min_work {
                    let work_ok = entry
                        .work_started_at
                        .map(|start| now_ms.saturating_sub(start) >= self.min_work_ms)
                        .unwrap_or(false);
                    // WaitingForInput follows real work: the turn_end marker
                    // or the waiting-tail heuristic can move the pane off
                    // Working before the session-log poll observes the
                    // settled turn. Requiring Working here made the faster
                    // signal permanently cannibalize the "finished" alert.
                    let was_active = matches!(
                        entry.status,
                        AgentActivityStatus::Working | AgentActivityStatus::WaitingForInput
                    );
                    if was_active && work_ok {
                        self.notify_locked(entry, NotifyKind::Finished, now_ms);
                    }
                } else {
                    self.notify_locked(entry, NotifyKind::Finished, now_ms);
                }
                entry.status = AgentActivityStatus::Completed;
                entry.work_started_at = None;
                true
            }
            AgentHistoryStatus::WaitingForInput => {
                if entry.status == AgentActivityStatus::WaitingForInput {
                    return false;
                }
                entry.status = AgentActivityStatus::WaitingForInput;
                self.notify_locked(entry, NotifyKind::NeedsAttention, now_ms);
                true
            }
            AgentHistoryStatus::Error => {
                if entry.status == AgentActivityStatus::Error {
                    return false;
                }
                entry.status = AgentActivityStatus::Error;
                self.notify_locked(entry, NotifyKind::Error, now_ms);
                true
            }
        }
    }

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
            // Include the run/session identity when available. For PTY-only
            // agents without a session id, the work start (or event timestamp)
            // distinguishes separate turns; a pane-wide key would suppress
            // every later completion after the first one.
            event_key: Some(format!(
                "agent:{}:{}:{}:{}",
                e.pane_id,
                kind.as_str(),
                e.session_id.as_deref().unwrap_or("unknown"),
                // Persistent TUIs reuse a session id across many turns. The
                // work-start identity is therefore part of the event key;
                // otherwise the first unresolved completion would suppress
                // every later completion in that session.
                e.work_started_at.unwrap_or(now_ms)
            )),
            run_id: e.session_id.clone(),
            pane_id: Some(e.pane_id.clone()),
            requires_action: matches!(kind, NotifyKind::NeedsAttention),
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
            e.generation,
        );
        if e.last_emitted.as_ref() == Some(&signature) {
            return;
        }
        self.update(pane_id, |entry| entry.last_emitted = Some(signature));

        let payload = serde_json::json!({
            "paneId": e.pane_id,
            "generation": e.generation,
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
                e.startup_pending = true;
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

    /// Evict a pane's tracking entry after its PTY is gone. Emits one final
    /// status (so the frontend clears any pill badge), then removes the map
    /// entry — without this, entries accumulate for the process lifetime.
    pub fn forget_pane(&self, pane_id: &str) {
        self.cancel_pane(pane_id, now_ms());
        self.panes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(pane_id);
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

    /// Belt-and-braces leak guard: silently evict entries whose last output
    /// predates `now_ms - max_idle_ms`. A pane whose PTY teardown path never
    /// ran (crashed reader, missed exit event) would otherwise persist
    /// forever. Deliberately NOT `forget_pane`: this is a janitor for panes
    /// that are almost certainly dead, not a user-visible cancellation, so
    /// no Cancelled alert fires and no final status is emitted. The caller
    /// owns the threshold (24 h per the audit plan); `idle_after_ms` (30 s)
    /// would evict live quiet panes every tick.
    /// Returns the evicted pane ids.
    pub fn prune_stale_panes(&self, now_ms: u64, max_idle_ms: u64) -> Vec<String> {
        let cutoff = now_ms.saturating_sub(max_idle_ms);
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        let mut guard = self.panes.lock().unwrap_or_else(|p| p.into_inner());
        let stale: Vec<String> = guard
            .values()
            .filter(|e| e.last_output_at > 0 && e.last_output_at < cutoff)
            .map(|e| e.pane_id.clone())
            .collect();
        for pane_id in &stale {
            guard.remove(pane_id);
        }
        stale
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
    fn pre_detection_output_does_not_mark_idle_agent_as_working() {
        // Startup banners arrive before the heartbeat classifies the
        // foreground process. They must not make an otherwise idle agent look
        // active; a later output pulse while the agent is known can still
        // promote it to Thinking/Working.
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
        t.heartbeat("p1", Some("claude"), None, None, 3000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
    }

    #[test]
    fn freshly_detected_agent_with_no_recent_output_stays_idle() {
        let (t, _events) = tracker();
        // Detection starts Idle; a later pulse while the agent is known will
        // promote to Thinking → Working.
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
    fn persistent_harness_session_log_marks_turn_completed_without_process_exit() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        let working = HistorySnapshot {
            task_title: "refactor the auth module".into(),
            session_id: "omp-1".into(),
            timestamp_ms: 1000,
            raw_prompt: "refactor the auth module".into(),
            activity: Some(AgentHistoryStatus::Working),
        };
        let completed = HistorySnapshot {
            activity: Some(AgentHistoryStatus::Completed),
            timestamp_ms: 2000,
            ..working.clone()
        };

        t.heartbeat("p1", Some("omp"), Some(&working), None, 1000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Working);
        t.heartbeat("p1", Some("omp"), Some(&completed), None, 2000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        assert_eq!(svc.get_history(None).len(), 1);
        assert_eq!(svc.get_history(None)[0].title, "Agent finished");
    }

    #[test]
    fn persistent_session_allows_multiple_turn_completion_notifications() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        let working_one = HistorySnapshot {
            task_title: "first task".into(),
            session_id: "persistent-session".into(),
            timestamp_ms: 1000,
            raw_prompt: "first task".into(),
            activity: Some(AgentHistoryStatus::Working),
        };
        let completed_one = HistorySnapshot {
            activity: Some(AgentHistoryStatus::Completed),
            timestamp_ms: 2000,
            ..working_one.clone()
        };
        let working_two = HistorySnapshot {
            task_title: "second task".into(),
            timestamp_ms: 3000,
            raw_prompt: "second task".into(),
            activity: Some(AgentHistoryStatus::Working),
            ..working_one.clone()
        };
        let completed_two = HistorySnapshot {
            activity: Some(AgentHistoryStatus::Completed),
            timestamp_ms: 4000,
            ..working_two.clone()
        };

        t.heartbeat("p1", Some("omp"), Some(&working_one), None, 1000);
        t.heartbeat("p1", Some("omp"), Some(&completed_one), None, 2000);
        t.heartbeat("p1", Some("omp"), Some(&working_two), None, 3000);
        t.heartbeat("p1", Some("omp"), Some(&completed_two), None, 4000);

        assert_eq!(svc.get_history(None).len(), 2);
    }

    #[test]
    fn authoritative_working_does_not_clear_permission_wait() {
        let (t, _events) = tracker();
        let working = HistorySnapshot {
            task_title: "run the migration".into(),
            session_id: "omp-2".into(),
            timestamp_ms: 1000,
            raw_prompt: "run the migration".into(),
            activity: Some(AgentHistoryStatus::Working),
        };
        t.heartbeat(
            "p1",
            Some("omp"),
            Some(&working),
            Some("Do you want to continue? (y/n)"),
            1000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
        t.heartbeat(
            "p1",
            Some("omp"),
            Some(&working),
            Some("Do you want to continue? (y/n)"),
            2500,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
    }

    #[test]
    fn waiting_state_does_not_cannibalize_finished_notification() {
        // The waiting-for-input signal (tail matcher or turn_end marker) is
        // faster than the session-log poll: by the time the settled turn is
        // observed, the pane is already WaitingForInput. Completion must
        // still produce the "finished" notification.
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        let working = HistorySnapshot {
            task_title: "task".into(),
            session_id: "omp-race".into(),
            timestamp_ms: 1000,
            raw_prompt: "task".into(),
            activity: Some(AgentHistoryStatus::Working),
        };
        let completed = HistorySnapshot {
            activity: Some(AgentHistoryStatus::Completed),
            timestamp_ms: 2000,
            ..working.clone()
        };

        t.heartbeat(
            "p1",
            Some("omp"),
            Some(&working),
            Some("Do you want to continue? (y/n)"),
            1000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
        t.heartbeat("p1", Some("omp"), Some(&completed), None, 2000);

        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        assert!(svc
            .get_history(None)
            .iter()
            .any(|n| n.title == "Agent finished"));
    }

    #[test]
    fn session_id_change_resets_completed_turn() {
        let (t, _events) = tracker();
        let h1 = HistorySnapshot {
            task_title: "task one".into(),
            session_id: "s1".into(),
            timestamp_ms: 1,
            raw_prompt: "task one".into(),
            activity: None,
        };
        let h2 = HistorySnapshot {
            task_title: "task two".into(),
            session_id: "s2".into(),
            timestamp_ms: 2,
            raw_prompt: "task two".into(),
            activity: None,
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
        // The first heartbeat clears launch-banner suppression; subsequent
        // output represents the user's actual turn.
        t.heartbeat("p1", Some("codex"), None, None, 1050);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.cancel_pane("p1", 1200);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Cancelled);
        let history = svc.get_history(None);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].title, "Agent cancelled");
    }

    #[test]
    fn forget_pane_evicts_entry_after_final_status() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.notify_agent_started("p1", "codex", 1000);
        t.heartbeat("p1", Some("codex"), None, None, 1050);
        t.on_pty_output("p1", 1100);
        t.on_pty_output("p1", 1101);
        t.forget_pane("p1");
        // Entry removed from the map entirely.
        assert!(t.pane_ids().is_empty());
        assert!(t.snapshot().is_empty());
        // Final cancellation status was still emitted before eviction.
        // get_history is newest-first.
        let history = svc.get_history(None);
        assert_eq!(
            history.first().map(|n| n.title.as_str()),
            Some("Agent cancelled")
        );
        t.forget_pane("never-registered");
    }

    #[test]
    fn prune_stale_panes_evicts_only_past_cutoff() {
        let (t, events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.notify_agent_started("quiet-agent", "codex", 1000);
        t.on_pty_output("quiet-agent", 86_400_000 + 5_000);
        t.heartbeat("fresh", None, None, None, 0);
        t.on_pty_output("fresh", 86_400_000 + 5_000);
        let history_len = svc.get_history(None).len();

        // 24 h cutoff at now = 24h + 20s: cutoff = 20 s. Both panes last
        // output 5 s after the 24 h mark → live, must SURVIVE.
        let evicted = t.prune_stale_panes(86_400_000 + 20_000, 24 * 60 * 60 * 1000);
        assert!(evicted.is_empty());
        assert!(t.pane_ids().contains(&"quiet-agent".to_string()));
        assert!(t.pane_ids().contains(&"fresh".to_string()));

        // A day later: both age out, silently — no new notifications, no
        // Cancelled alert for the agent pane, no state churn.
        let evicted = t.prune_stale_panes(2 * 86_400_000 + 20_000, 24 * 60 * 60 * 1000);
        assert_eq!(evicted.len(), 2);
        assert!(t.pane_ids().is_empty());
        assert_eq!(svc.get_history(None).len(), history_len);
        let emitted = events.lock().unwrap().len();

        // Never-output panes (last_output_at == 0) are not evicted by time.
        t.heartbeat("silent", None, None, None, 0);
        assert!(t
            .prune_stale_panes(90_000_000, 24 * 60 * 60 * 1000)
            .is_empty());
        assert!(t.pane_ids().contains(&"silent".to_string()));
        assert_eq!(events.lock().unwrap().len(), emitted);
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

    fn lifecycle_event(kind: AgentLifecycleKind, agent: Option<&str>) -> AgentLifecycleEvent {
        AgentLifecycleEvent {
            kind,
            agent: agent.map(str::to_string),
            session_id: None,
            message: None,
        }
    }

    #[test]
    fn pushed_complete_notifies_immediately_without_prior_working() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        // The heartbeat never observed the agent working, yet an explicit
        // push must still fire a "finished" notification — the min-work gate
        // is bypassed for agent-pushed signals.
        t.on_agent_lifecycle(
            "p1",
            &lifecycle_event(AgentLifecycleKind::Complete, Some("claude")),
            None,
            1000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        let history = svc.get_history(None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].title, "Agent finished");
    }

    #[test]
    fn pushed_request_notifies_needs_attention() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.on_agent_lifecycle(
            "p1",
            &lifecycle_event(AgentLifecycleKind::Request, Some("claude")),
            None,
            1000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::WaitingForInput);
        let history = svc.get_history(None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].r#type, NotificationType::NeedsInput);
        assert!(history[0].requires_action);
    }

    #[test]
    fn pushed_error_notifies_error() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        t.on_agent_lifecycle(
            "p1",
            &lifecycle_event(AgentLifecycleKind::Error, Some("codex")),
            None,
            1000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Error);
        let history = svc.get_history(None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].r#type, NotificationType::TaskError);
    }

    #[test]
    fn pushed_lifecycle_is_generation_aware() {
        let (t, _events) = tracker();
        let old_gen = t.register_pane_with_generation("p1");
        let new_gen = t.register_pane_with_generation("p1");
        let ev = lifecycle_event(AgentLifecycleKind::Complete, Some("claude"));
        // A stale read loop cannot move the newer pane.
        t.on_agent_lifecycle("p1", &ev, Some(old_gen), 1000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        // The current loop can.
        t.on_agent_lifecycle("p1", &ev, Some(new_gen), 1000);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
    }

    #[test]
    fn pushed_lifecycle_adopts_agent_key_for_label() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        // No heartbeat classified the pane yet; the event's agent key is
        // adopted so the notification carries the real label.
        t.on_agent_lifecycle(
            "p1",
            &lifecycle_event(AgentLifecycleKind::Request, Some("freebuff")),
            None,
            1000,
        );
        let history = svc.get_history(None);
        assert_eq!(history.len(), 1);
        assert!(history[0].message.contains("Freebuff"));
    }

    #[test]
    fn pushed_complete_is_idempotent() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        let ev = lifecycle_event(AgentLifecycleKind::Complete, Some("claude"));
        t.on_agent_lifecycle("p1", &ev, None, 1000);
        t.on_agent_lifecycle("p1", &ev, None, 1100);
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Completed);
        assert_eq!(svc.get_history(None).len(), 1);
    }

    #[test]
    fn pushed_status_ignored_when_plugin_connected() {
        let (t, _events) = tracker();
        let svc = t.notifications.clone().unwrap();
        // The pane must be tracked first: set_plugin_connected only updates an
        // existing entry (the PTY read loop registers the pane at spawn).
        t.register_pane("p1");
        t.set_plugin_connected("p1", true);
        t.on_agent_lifecycle(
            "p1",
            &lifecycle_event(AgentLifecycleKind::Request, Some("claude")),
            None,
            1000,
        );
        assert_eq!(status_of(&t, "p1"), AgentActivityStatus::Idle);
        assert_eq!(svc.get_history(None).len(), 0);
    }
}
