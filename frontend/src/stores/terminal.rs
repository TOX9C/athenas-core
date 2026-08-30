use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::workspace::terminal_input::TerminalInputRouter;
use crate::tauri_bridge;

#[path = "terminal_colors.rs"]
mod terminal_colors;

pub use terminal_colors::{
    backend_color_raw_to_terminal, backend_named_color_to_terminal, BackendColorIndexed,
    BackendColorNamed, BackendColorRaw, BackendColorRgb, BackendNamedColor, TerminalColor,
};

// ---------------------------------------------------------------------------
// Terminal cell model (mirrors athena-terminal crate)
// ---------------------------------------------------------------------------

/// A single terminal cell with text, color, and style info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TerminalCell {
    pub text: String,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub inverse: bool,
    pub strikethrough: bool,
}

impl TerminalCell {
    /// Convert a backend CellDelta (parsed from JSON) into a TerminalCell.
    pub fn from_delta(delta: &CellDeltaEvent) -> Self {
        Self {
            text: delta.c.clone(),
            fg: backend_color_raw_to_terminal(&delta.fg),
            bg: backend_color_raw_to_terminal(&delta.bg),
            bold: (delta.flags & FLAGS_BOLD) != 0,
            italic: (delta.flags & FLAGS_ITALIC) != 0,
            underline: (delta.flags & FLAGS_UNDERLINE) != 0,
            blink: (delta.flags & FLAGS_BLINK) != 0,
            inverse: (delta.flags & FLAGS_INVERSE) != 0,
            strikethrough: (delta.flags & FLAGS_STRIKEOUT) != 0,
        }
    }
}

#[path = "terminal_events.rs"]
mod terminal_events;

pub use terminal_events::{
    CellDeltaEvent, TerminalDataEvent, TerminalUpdateDelta, FLAGS_BLINK, FLAGS_BOLD, FLAGS_INVERSE,
    FLAGS_ITALIC, FLAGS_STRIKEOUT, FLAGS_UNDERLINE,
};

/// A single terminal row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalRow {
    pub cells: Vec<TerminalCell>,
    pub dirty: bool,
    pub wrapped: bool,
}

/// State for a single terminal session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalSession {
    pub id: String,
    pub grid: Vec<Vec<TerminalCell>>,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: Vec<Vec<TerminalCell>>,
    pub scrollback_offset: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub cursor_visible: bool,
    pub is_ready: bool,
    pub cwd: String,
    /// Which of the top rows are dirty (needs redraw).
    pub dirty_rows: std::collections::HashSet<usize>,
    /// Incremented on every change; triggers use_memo/use_effect in components.
    pub generation: u64,
    /// Timestamp of last data received (for dirty streak debouncing).
    pub last_update_ms: f64,
    /// Whether the session has exited.
    pub exited: bool,
    /// Whether the session is rendered by xterm.js (skip grid updates).
    pub is_xterm: bool,
    /// Detected foreground process name (e.g., "claude", "codex", "nvim")
    pub foreground_process: Option<String>,
    /// Detected agent task title (e.g. "Fix login bug"), scraped from the agent's state files.
    pub task_title: Option<String>,
    /// Session ID from the agent's history file (used to avoid re-summarizing).
    pub session_id: Option<String>,
    /// Raw prompt text (available for LLM summarization).
    pub raw_prompt: Option<String>,
    /// Per-pane title state machine. Idle until a prompt is scraped, Pending
    /// while the LLM call is in flight, Failed if it exhausted retries, Done
    /// once a title (or "Sensitive prompt") is available. See utils/pane_label.
    pub title_state: crate::utils::pane_label::TitleState,
    /// VT escape string produced by `@xterm/addon-serialize` just before the
    /// xterm.js Terminal is disposed on a pane swap / unmount. The next mount's
    /// reuse-session branch writes this back into the fresh Terminal (after
    /// fit()), restoring colors, scrollback, alt-screen state, DEC modes, and
    /// cursor position. `None` for non-xterm sessions or when no capture ran.
    /// See xterm_mount.rs `serialize_buffer` / the use_drop capture hook.
    pub serialized_snapshot: Option<String>,
}

impl TerminalSession {
    /// Create a blank session with the given dimensions.
    pub fn new(id: impl Into<String>, cols: u16, rows: u16) -> Self {
        let id = id.into();
        let grid = vec![vec![TerminalCell::default(); cols as usize]; rows as usize];
        Self {
            id,
            grid,
            cols,
            rows,
            scrollback: Vec::new(),
            scrollback_offset: 0,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            is_ready: false,
            cwd: String::new(),
            dirty_rows: std::collections::HashSet::new(),
            generation: 0,
            last_update_ms: 0.0,
            exited: false,
            is_xterm: false,
            foreground_process: None,
            task_title: None,
            session_id: None,
            raw_prompt: None,
            title_state: crate::utils::pane_label::TitleState::default(),
            serialized_snapshot: None,
        }
    }

    /// Resize the grid while preserving existing content.
    pub fn resize(&mut self, new_cols: u16, new_rows: u16) {
        if self.cols == new_cols && self.rows == new_rows {
            return;
        }
        let mut new_grid =
            vec![vec![TerminalCell::default(); new_cols as usize]; new_rows as usize];
        for (y, row) in self.grid.iter().enumerate() {
            if y >= new_rows as usize {
                break;
            }
            for (x, cell) in row.iter().enumerate() {
                if x >= new_cols as usize {
                    break;
                }
                new_grid[y][x] = cell.clone();
            }
        }
        self.grid = new_grid;
        self.cols = new_cols;
        self.rows = new_rows;
        self.mark_grid_dirty();
    }

    /// Mark all rows dirty.
    pub fn mark_grid_dirty(&mut self) {
        self.dirty_rows.clear();
        for y in 0..self.rows as usize {
            self.dirty_rows.insert(y);
        }
    }

    /// Clear dirty flags after a render cycle.
    pub fn clear_dirty(&mut self) {
        self.dirty_rows.clear();
    }

    /// Update cells from a backend delta.
    pub fn apply_delta(&mut self, delta: TerminalUpdateDelta) {
        for (y, row) in delta.rows.into_iter().enumerate() {
            let grid_y = delta.start_y + y;
            if grid_y >= self.grid.len() {
                continue;
            }
            for (x, cell) in row.into_iter().enumerate() {
                if x >= self.grid[grid_y].len() {
                    break;
                }
                self.grid[grid_y][x] = cell;
            }
            self.dirty_rows.insert(grid_y);
        }
        if let Some((cx, cy)) = delta.cursor_pos {
            self.cursor_x = cx;
            self.cursor_y = cy;
        }
        self.generation = self.generation.wrapping_add(1);
        self.last_update_ms = js_sys::Date::now();
    }
}

// ---------------------------------------------------------------------------
// TerminalStore — global state for all terminal sessions
// ---------------------------------------------------------------------------

/// Global terminal store holding *whole-store* terminal state.
///
/// Phase 4 (Item 3): per-pane `TerminalSession` data lives entirely in the
/// context-provided `TerminalRegistry`'s per-session signals; this store only
/// tracks **membership** (`known_pane_ids`) and the cross-session **active id**.
/// `generation` is bumped ONLY for cross-session invalidation (membership
/// add/remove, active-id change, exit) — never per `terminal:data` event.
/// This is what stops a single pane's foreground/cell update from re-rendering
/// every other pane.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalStore {
    /// Pane ids known to be alive (mirrors the registry's keys). Used for O(1)
    /// membership checks (`contains_key`) and close-fallback active selection.
    pub known_pane_ids: HashSet<String>,
    pub active_session_id: Option<String>,
    pub generation: u64,
}

// -- Actions backed by the Tauri backend ----------------------------------

impl TerminalStore {
    /// Ensure a session exists without doing any async work.
    /// Returns true if a new session was inserted.
    ///
    /// Phase 4 (Item 3): the per-pane `TerminalSession` data is created in the
    /// registry's inner signal; this store records the pane id in
    /// `known_pane_ids` (membership, O(1)) and bumps `generation` for
    /// cross-session invalidation, and sets the active id if none is set.
    pub fn ensure_session(
        &mut self,
        registry: &TerminalRegistry,
        id: impl Into<String>,
        cols: u16,
        rows: u16,
    ) -> bool {
        let id_str: String = id.into();
        let was_new = registry.ensure_session(id_str.clone(), cols, rows);
        if was_new {
            self.known_pane_ids.insert(id_str.clone());
            if self.active_session_id.is_none() {
                self.active_session_id = Some(id_str);
            }
            self.generation = self.generation.wrapping_add(1);
        }
        was_new
    }

    /// Mark a session as being backed by xterm.js (or not).
    /// Phase 4: per-pane only — writes the inner signal; the slimmed store
    /// tracks only membership/active, which `is_xterm` doesn't change.
    pub fn set_session_xterm(&mut self, registry: &TerminalRegistry, id: &str, is_xterm: bool) {
        if let Some(mut inner) = registry.write_session(id) {
            inner.is_xterm = is_xterm;
        }
    }

    /// Fire the async PTY bridge call (does NOT touch store state).
    pub async fn spawn_bridge(
        &self,
        id: impl Into<String>,
        cwd: &str,
        shell: &str,
        cols: u16,
        rows: u16,
    ) {
        let id_str: String = id.into();
        // test- sessions are local-only
        if id_str.starts_with("test-") {
            return;
        }
        if let Err(e) = tauri_bridge::pty_spawn(&id_str, cwd, shell, cols, rows, false, None).await
        {
            web_sys::console::warn_1(&format!("pty_spawn backend call failed: {:?}", e).into());
        }
    }

    /// Write data to an active PTY session.
    pub async fn write(&self, id: &str, data: &str) {
        if let Err(e) = tauri_bridge::pty_write(id, data).await {
            web_sys::console::error_1(&format!("pty_write failed: {:?}", e).into());
        }
    }

    /// Kill a PTY session.
    /// Phase 4: removes from registry + `known_pane_ids`, reassigns active if
    /// needed (fallback to another known pane), bumps `generation` (membership).
    pub async fn kill(&mut self, registry: &TerminalRegistry, id: &str) {
        if let Err(e) = tauri_bridge::pty_kill(id).await {
            web_sys::console::error_1(&format!("pty_kill failed: {:?}", e).into());
        }
        registry.remove(id);
        self.known_pane_ids.remove(id);
        if self.active_session_id.as_deref() == Some(id) {
            self.active_session_id = self.known_pane_ids.iter().next().cloned();
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// Resize a PTY session.
    /// Phase 4: per-pane grid resize lives on the inner signal only;
    /// the backend `pty_resize` IPC call is unchanged.
    pub async fn resize(&mut self, registry: &TerminalRegistry, id: &str, cols: u16, rows: u16) {
        if let Some(mut inner) = registry.write_session(id) {
            inner.resize(cols, rows);
        }
        if let Err(e) = tauri_bridge::pty_resize(id, cols, rows, None).await {
            web_sys::console::error_1(&format!("pty_resize failed: {:?}", e).into());
        }
    }

    /// Handle incoming data from the backend.
    ///
    /// Phase 3 (Item 3): the per-pane state lives in the registry's inner
    /// `Signal<TerminalSession>`. `on_data` writes that inner signal ONLY — no
    /// whole-store `generation` bump — so a `terminal:data` event for pane A
    /// re-renders only pane A's subscribers (the hot path, up to ~125/sec/pane).
    /// The legacy `self.sessions` map is no longer touched here. (Phase 4 will
    /// remove `self.sessions` entirely.)
    pub fn on_data(&mut self, registry: &TerminalRegistry, id: &str, payload: &str) {
        let event: TerminalDataEvent = match serde_json::from_str(payload) {
            Ok(e) => e,
            Err(err) => {
                web_sys::console::error_1(
                    &format!("terminal:data parse error for session {}: {}", id, err).into(),
                );
                return;
            }
        };

        let Some(mut inner) = registry.write_session(id) else {
            // Pane not registered (e.g. closed between the PTY event and this
            // write). Nothing to update — no fallback store read either, since
            // the registry is now the source of truth for per-pane data.
            return;
        };

        // For xterm-managed sessions, skip grid updates and the generation bump.
        // Cursor position and visibility are still updated in case other UI reads
        // them (e.g. restore_term_from_session).
        if inner.is_xterm {
            inner.cursor_x = event.cursorCol;
            inner.cursor_y = event.cursorRow;
            if let Some(visible) = event.cursorVisible {
                inner.cursor_visible = visible;
            }
            return;
        }

        // Resize grid if the backend reports different dimensions.
        if event.rows as u16 != inner.rows || event.cols as u16 != inner.cols {
            inner.resize(event.cols as u16, event.rows as u16);
        }

        // Apply each cell delta to the grid.
        for delta in &event.deltas {
            let row = delta.row;
            let col = delta.col;
            if row < inner.grid.len() && col < inner.grid[row].len() {
                inner.grid[row][col] = TerminalCell::from_delta(delta);
                inner.dirty_rows.insert(row);
            }
        }

        // Update cursor position and visibility.
        inner.cursor_x = event.cursorCol;
        inner.cursor_y = event.cursorRow;
        if let Some(visible) = event.cursorVisible {
            inner.cursor_visible = visible;
        }

        inner.generation = inner.generation.wrapping_add(1);
        inner.last_update_ms = js_sys::Date::now();
        // No whole-store `self.generation` bump: pane subscribers already
        // invalidated by the inner-signal write above; other panes stay idle.
    }

    /// Handle session exit from the backend.
    pub fn on_exit(&mut self, registry: &TerminalRegistry, id: &str) {
        if let Some(mut inner) = registry.write_session(id) {
            inner.exited = true;
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// Update the detected foreground process + scraped task title for a pane.
    ///
    /// Called on a slow timer (the central `AgentInfoPoller`) for every pane.
    /// Phase 5 (Item 3): writes the per-pane inner signal ONLY — no whole-store
    /// `generation` bump — so a foreground/title change in one pane doesn't
    /// invalidate other panes' subscribers.
    ///
    /// **Change detection restored at the registry level.** A `Signal::write()`
    /// guard's drop marks the signal's subscribers dirty *unconditionally*
    /// (dioxus-signals 0.7.9 `SignalSubscriberDrop::drop` calls
    /// `update_subscribers()` on every write, not only on value change). So we
    /// `peek()` (non-subscribing) first and only take a write guard when an
    /// observable field (`foreground_process`/`task_title`) or a stored field
    /// (`session_id`/`raw_prompt`) actually differs — otherwise an identical
    /// 1500ms poll would re-evaluate this pane's pill memo every cycle.
    pub fn update_agent_info(
        &mut self,
        registry: &TerminalRegistry,
        id: &str,
        fg_process: Option<String>,
        task_title: Option<String>,
        session_id: Option<String>,
        raw_prompt: Option<String>,
    ) {
        // Non-subscribing snapshot to decide whether a write is needed at all.
        let Some(current) = registry.peek_session(id) else {
            // Pane not registered (closed between poll and write): drop the info.
            return;
        };
        let fg_changed = current.foreground_process != fg_process;
        let title_changed = current.task_title != task_title;
        let sid_changed = session_id
            .as_ref()
            .is_some_and(|sid| current.session_id.as_deref() != Some(sid.as_str()));
        let prompt_changed = raw_prompt
            .as_ref()
            .is_some_and(|p| current.raw_prompt.as_deref() != Some(p.as_str()));
        if !(fg_changed || title_changed || sid_changed || prompt_changed) {
            return;
        }

        if let Some(mut inner) = registry.write_session(id) {
            if fg_changed || title_changed {
                inner.foreground_process = fg_process;
                inner.task_title = task_title;
                // Inner `generation` is bump-only/never-read; the signal write
                // itself invalidates the pill memo, but keep the bump for parity
                // with the legacy field's documented semantics.
                inner.generation = inner.generation.wrapping_add(1);
            }
            if let Some(sid) = session_id {
                inner.session_id = Some(sid);
            }
            if let Some(prompt) = raw_prompt {
                inner.raw_prompt = Some(prompt);
            }
        }
    }

    /// Activate a session by ID.
    pub fn set_active(&mut self, id: impl Into<String>) {
        let id = id.into();
        if self.active_session_id.as_ref() != Some(&id) {
            self.active_session_id = Some(id);
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalRegistry — per-session reactive signals (decomposition Item 3)
// ---------------------------------------------------------------------------

/// A per-pane reactive signal registry.
///
/// `TerminalStore` holds the *whole-store* state (membership, active id) behind
/// a single `Signal<TerminalStore>`, so every `.read()` subscribes to every
/// change. A single pane's foreground/title update bumps the whole-store
/// `generation`, re-rendering every `PaneItem` memo on every `terminal:data`
/// event (up to ~125/sec/pane).
///
/// The registry holds a lazily-created `Signal<TerminalSession>` **per pane**.
/// Components fetch the registry via context (a free lookup, no subscription),
/// then `.read()` one pane's inner signal → subscribe to *only* that pane. A
/// change in pane A no longer invalidates pane B's subscribers.
///
/// `Rc<RefCell<…>>` is the WASM-safe interior-mutability choice already used in
/// `xterm_mount.rs` and `output_event_bus.rs` (single-threaded WASM, no `Send`
/// requirement).
#[derive(Clone)]
pub struct TerminalRegistry {
    sessions: Rc<RefCell<HashMap<String, Signal<TerminalSession>>>>,
    input_router: TerminalInputRouter,
    // Pane ids marked for permanent close. XtermMount consumes this marker in
    // its drop hook, after unlisten/snapshot/dispose, so registry removal never
    // races component teardown or a temporary remount.
    closing: Rc<RefCell<HashSet<String>>>,
}

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            sessions: Rc::new(RefCell::new(HashMap::new())),
            input_router: TerminalInputRouter::new(),
            closing: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    /// Lazily create the inner `Signal<TerminalSession>` for `id` if absent.
    /// Returns `true` if a new signal was inserted.
    ///
    /// The signal is bound to `ScopeId::APP` (the app-root scope, app-lifetime)
    /// via `Signal::new_in_scope`, NOT to whatever scope is current when this
    /// runs. `ensure_session` is typically called from inside `XtermMount`'s
    /// `spawn(async move {...})` block. `Signal::new` would bind the signal's
    /// storage to the scope active at that moment; when `XtermMount` later
    /// unmounts (e.g. a pane-pill drag-swap changes the `xterm-{pane_id}`
    /// keyed element, or a panel switch remounts the grid), Dioxus reclaims
    /// that scope's owned signals — but the `TerminalRegistry` (held by the
    /// app-root context) still references the now-dropped signal. The next
    /// `PaneItem`/`TerminalPaneBody` `use_memo` that reads it panics with
    /// `Dropped(ValueDroppedError)` at `terminal_grid.rs:386`. Anchoring to
    /// `ScopeId::APP` keeps the signal alive for the whole app regardless of
    /// which component created or later unmounted it. (Same root-cause family
    /// as the prior panel-switch ValueDropped panic; fix per dioxus-core's own
    /// `ScopeId::APP` guidance for long-lived dynamic state.)
    pub fn ensure_session(&self, id: impl Into<String>, cols: u16, rows: u16) -> bool {
        let id_str: String = id.into();
        let mut map = self.sessions.borrow_mut();
        if map.contains_key(&id_str) {
            return false;
        }
        map.insert(
            id_str.clone(),
            Signal::new_in_scope(TerminalSession::new(id_str, cols, rows), ScopeId::APP),
        );
        true
    }

    /// Shared serialized input router used by xterm and file-drop input.
    pub fn input_router(&self) -> TerminalInputRouter {
        self.input_router.clone()
    }

    /// Returns the reactive per-session signal, or `None` if no session for `id`.
    pub fn session_signal(&self, id: &str) -> Option<Signal<TerminalSession>> {
        self.sessions.borrow().get(id).cloned()
    }

    /// Returns a write guard for the session's inner signal, or `None`.
    /// Dropping the guard invalidates *only* this pane's subscribers.
    pub fn write_session(&self, id: &str) -> Option<WritableRef<'static, Signal<TerminalSession>>> {
        // `write_unchecked` takes `&self` (Signal is `Copy`-like via
        // generational-box) and returns a `'static` guard, sidestepping the
        // borrow-guard-vs-return-lifetime conflict that `Signal::write()` (which
        // needs `&mut self`) creates. Runtime borrow checking still applies.
        let signal = self.sessions.borrow().get(id).cloned()?;
        Some(signal.write_unchecked())
    }

    /// Read the session without subscribing (one-shot snapshots).
    pub fn peek_session(&self, id: &str) -> Option<TerminalSession> {
        let signal = self.sessions.borrow().get(id).cloned()?;
        let guard = signal.peek();
        Some((*guard).clone())
    }
    /// Mark a pane for permanent close. The xterm component consumes this
    /// marker from its drop hook after native resources are disposed.
    pub fn mark_closing(&self, id: &str) {
        self.closing.borrow_mut().insert(id.to_string());
    }

    /// Cancel a pending close when backend teardown fails and the pane remains.
    pub fn cancel_closing(&self, id: &str) {
        self.closing.borrow_mut().remove(id);
    }

    /// Returns whether a pane is being permanently closed rather than
    /// temporarily remounted during a layout/swap.
    pub fn is_closing(&self, id: &str) -> bool {
        self.closing.borrow().contains(id)
    }

    /// Remove a session's signal (on kill / close-pane).
    pub fn remove(&self, id: &str) {
        self.sessions.borrow_mut().remove(id);
        self.closing.borrow_mut().remove(id);
    }

    /// Snapshot of known pane ids (e.g. for close-fallback active selection).
    pub fn known_ids(&self) -> Vec<String> {
        self.sessions.borrow().keys().cloned().collect()
    }

    /// Is a session registered for `id`?
    pub fn contains(&self, id: &str) -> bool {
        self.sessions.borrow().contains_key(id)
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the terminal store from Dioxus context.
pub fn use_terminal_store() -> Signal<TerminalStore> {
    use_context::<Signal<TerminalStore>>()
}

/// Obtain the per-session terminal registry from Dioxus context.
pub fn use_terminal_registry() -> TerminalRegistry {
    use_context::<TerminalRegistry>()
}

/// Returns the reactive per-session signal for `id`, if registered.
pub fn use_session_signal(id: &str) -> Option<Signal<TerminalSession>> {
    use_terminal_registry().session_signal(id)
}

/// Initialize the terminal store and per-session registry as context providers.
pub fn provide_terminal_store() {
    use_context_provider(|| Signal::new(TerminalStore::default()));
    use_context_provider(TerminalRegistry::new);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::VirtualDom;

    // Per-test body stashed in a thread-local so the root component closure —
    // which `VirtualDom::new` requires as a non-capturing `fn()`
    // pointer — can still access it. The body runs once on rebuild inside the
    // live signal runtime, then is cleared for the next test.
    thread_local! {
        static PENDING_BODY:
            std::cell::RefCell<
                Option<Box<dyn FnOnce(&TerminalRegistry)>>,
            > = const { std::cell::RefCell::new(None) };
    }

    /// Run `body` inside a fresh `VirtualDom` so signals attach to a live
    /// runtime. Dioxus signals panic outside one, so the `VirtualDom` harness is
    /// required even for unit-level registry tests (mirrors how dioxus-signals'
    /// own tests bootstrap — `tests/create.rs`).
    fn run_in_dom(body: impl FnOnce(&TerminalRegistry) + 'static) {
        PENDING_BODY.with(|cell| cell.replace(Some(Box::new(body))));
        let mut dom = VirtualDom::new(|| {
            use_context_provider(|| Signal::new(TerminalStore::default()));
            let registry: TerminalRegistry = use_context_provider(TerminalRegistry::new);
            PENDING_BODY.with(|cell| {
                if let Some(b) = cell.borrow_mut().take() {
                    b(&registry);
                }
            });
            rsx! {}
        });
        dom.rebuild_to_vec();
    }

    #[test]
    fn ensure_session_inserts_once_and_reports_new() {
        run_in_dom(|r| {
            assert!(r.ensure_session("pane-a", 80, 24), "first insert is new");
            assert!(!r.ensure_session("pane-a", 80, 24), "second insert not new");
            assert!(r.contains("pane-a"));
            assert!(!r.contains("pane-b"));
        });
    }

    #[test]
    fn session_signal_round_trips_after_ensure() {
        run_in_dom(|r| {
            assert!(r.session_signal("pane-x").is_none(), "absent before ensure");
            r.ensure_session("pane-x", 80, 24);
            let sig = r.session_signal("pane-x").expect("present after ensure");
            // Non-subscribing peek returns the seeded dimensions.
            let snap = r.peek_session("pane-x").unwrap();
            assert_eq!(snap.id, "pane-x");
            assert_eq!(snap.cols, 80);
            assert_eq!(snap.rows, 24);
            // The signal reads back the same id.
            assert_eq!(sig.read().id, "pane-x");
        });
    }

    #[test]
    fn write_session_mutates_only_this_pane() {
        run_in_dom(|r| {
            r.ensure_session("pane-a", 80, 24);
            r.ensure_session("pane-b", 80, 24);
            if let Some(mut a) = r.write_session("pane-a") {
                a.is_xterm = true;
                a.foreground_process = Some("claude".to_string());
            }
            assert!(r.peek_session("pane-a").unwrap().is_xterm);
            assert_eq!(
                r.peek_session("pane-a")
                    .unwrap()
                    .foreground_process
                    .as_deref(),
                Some("claude"),
            );
            // pane-b untouched
            assert!(!r.peek_session("pane-b").unwrap().is_xterm);
            assert_eq!(r.peek_session("pane-b").unwrap().foreground_process, None,);
        });
    }

    #[test]
    fn closing_marker_survives_until_drop_cleanup() {
        run_in_dom(|r| {
            r.ensure_session("pane-a", 80, 24);
            r.mark_closing("pane-a");
            assert!(r.is_closing("pane-a"));
            // A remount path can cancel a close before resources are dropped.
            r.cancel_closing("pane-a");
            assert!(!r.is_closing("pane-a"));
            r.mark_closing("pane-a");
            r.remove("pane-a");
            assert!(!r.is_closing("pane-a"));
            assert!(!r.contains("pane-a"));
        });
    }

    #[test]
    fn remove_drops_session_and_known_ids() {
        run_in_dom(|r| {
            r.ensure_session("pane-a", 80, 24);
            r.ensure_session("pane-b", 80, 24);
            assert_eq!(r.known_ids().len(), 2);
            r.remove("pane-a");
            assert!(!r.contains("pane-a"));
            assert!(r.session_signal("pane-a").is_none());
            assert!(r.contains("pane-b"));
        });
    }

    #[test]
    fn write_session_for_missing_pane_is_none() {
        run_in_dom(|r| {
            assert!(r.write_session("ghost").is_none());
            assert!(r.peek_session("ghost").is_none());
        });
    }
}
