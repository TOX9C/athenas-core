use std::collections::HashMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::tauri_bridge;

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

/// Terminal color (ANSI 256 + true color support).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerminalColor {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Default for TerminalColor {
    fn default() -> Self {
        TerminalColor::Default
    }
}

// ---------------------------------------------------------------------------
// Backend NamedColor enum (mirrors athena-terminal NamedColor)
// ---------------------------------------------------------------------------

/// Mirrors the backend's NamedColor enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackendNamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground,
    Background,
}

/// Convert a BackendNamedColor into the frontend's TerminalColor.
pub fn backend_named_color_to_terminal(nc: &BackendNamedColor) -> TerminalColor {
    match nc {
        BackendNamedColor::Black => TerminalColor::Black,
        BackendNamedColor::Red => TerminalColor::Red,
        BackendNamedColor::Green => TerminalColor::Green,
        BackendNamedColor::Yellow => TerminalColor::Yellow,
        BackendNamedColor::Blue => TerminalColor::Blue,
        BackendNamedColor::Magenta => TerminalColor::Magenta,
        BackendNamedColor::Cyan => TerminalColor::Cyan,
        BackendNamedColor::White => TerminalColor::White,
        BackendNamedColor::BrightBlack => TerminalColor::BrightBlack,
        BackendNamedColor::BrightRed => TerminalColor::BrightRed,
        BackendNamedColor::BrightGreen => TerminalColor::BrightGreen,
        BackendNamedColor::BrightYellow => TerminalColor::BrightYellow,
        BackendNamedColor::BrightBlue => TerminalColor::BrightBlue,
        BackendNamedColor::BrightMagenta => TerminalColor::BrightMagenta,
        BackendNamedColor::BrightCyan => TerminalColor::BrightCyan,
        BackendNamedColor::BrightWhite => TerminalColor::BrightWhite,
        BackendNamedColor::Foreground => TerminalColor::Default,
        BackendNamedColor::Background => TerminalColor::Default,
    }
}

// ---------------------------------------------------------------------------
// Backend event types for `terminal:data`
// ---------------------------------------------------------------------------

/// Parsed payload of a `terminal:data` event from the Tauri backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct TerminalDataEvent {
    pub sessionId: String,
    pub deltas: Vec<CellDeltaEvent>,
    pub cursorRow: usize,
    pub cursorCol: usize,
    pub rows: usize,
    pub cols: usize,
    pub cursorVisible: Option<bool>,
}

/// A single cell delta from the backend (mirrors `CellDelta`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellDeltaEvent {
    pub row: usize,
    pub col: usize,
    pub c: String,
    pub fg: BackendColorRaw,
    pub bg: BackendColorRaw,
    pub flags: u16,
}

/// Raw representation of the backend `Color` enum as it arrives in JSON.
///
/// The backend uses externally-tagged serde (the default), so:
/// - `"Default"`                   → `BackendColorRaw::Default`
/// - `{"Named": "Red"}`           → `BackendColorRaw::Named("Red")`
/// - `{"Indexed": 128}`           → `BackendColorRaw::Indexed(128)`
/// - `{"Rgb": [255, 128, 0]}`     → `BackendColorRaw::Rgb([255, 128, 0])`
///
/// We use an untagged enum to handle both the bare-string and object variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackendColorRaw {
    /// `"Default"` — the only variant serialized as a bare string.
    DefaultString(String),
    /// `{"Named": "Red"}` etc.
    Named(BackendColorNamed),
    /// `{"Indexed": 128}`
    Indexed(BackendColorIndexed),
    /// `{"Rgb": [255, 128, 0]}`
    Rgb(BackendColorRgb),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct BackendColorNamed {
    pub Named: BackendNamedColor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct BackendColorIndexed {
    pub Indexed: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct BackendColorRgb {
    pub Rgb: [u8; 3],
}

/// Convert a `BackendColorRaw` (parsed from JSON) into the frontend's
/// `TerminalColor`.
pub fn backend_color_raw_to_terminal(raw: &BackendColorRaw) -> TerminalColor {
    match raw {
        BackendColorRaw::DefaultString(s) if s == "Default" => TerminalColor::Default,
        BackendColorRaw::DefaultString(s) => {
            web_sys::console::warn_1(&format!("unknown bare color string: {}", s).into());
            TerminalColor::Default
        }
        BackendColorRaw::Named(n) => backend_named_color_to_terminal(&n.Named),
        BackendColorRaw::Indexed(i) => TerminalColor::Indexed(i.Indexed),
        BackendColorRaw::Rgb(rgb) => TerminalColor::Rgb(rgb.Rgb[0], rgb.Rgb[1], rgb.Rgb[2]),
    }
}

// ---------------------------------------------------------------------------
// CellFlags bit constants (mirrors athena-terminal CellFlags)
// ---------------------------------------------------------------------------

/// Bit 0: INVERSE
pub const FLAGS_INVERSE: u16 = 0b0000_0000_0000_0001;
/// Bit 1: BOLD
pub const FLAGS_BOLD: u16 = 0b0000_0000_0000_0010;
/// Bit 2: ITALIC
pub const FLAGS_ITALIC: u16 = 0b0000_0000_0000_0100;
/// Bit 3: UNDERLINE
pub const FLAGS_UNDERLINE: u16 = 0b0000_0000_0000_1000;
/// Bit 7: STRIKEOUT
pub const FLAGS_STRIKEOUT: u16 = 0b0000_0000_1000_0000;
/// Bit 8: BLINK
pub const FLAGS_BLINK: u16 = 0b0000_0001_0000_0000;

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
    /// LLM-generated short title (2-3 words). Only populated when the feature is enabled.
    pub summarized_title: Option<String>,
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
            summarized_title: None,
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

/// Delta update from the backend (mirrors athena-terminal TerminalUpdate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TerminalUpdateDelta {
    pub start_y: usize,
    pub rows: Vec<Vec<TerminalCell>>,
    pub cursor_pos: Option<(usize, usize)>,
}

// ---------------------------------------------------------------------------
// TerminalStore — global state for all terminal sessions
// ---------------------------------------------------------------------------

/// Global terminal store holding all active sessions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalStore {
    pub sessions: HashMap<String, TerminalSession>,
    pub active_session_id: Option<String>,
    pub generation: u64,
}

// -- Actions backed by the Tauri backend ----------------------------------

impl TerminalStore {
    /// Ensure a session exists without doing any async work.
    /// Returns true if a new session was inserted.
    pub fn ensure_session(&mut self, id: impl Into<String>, cols: u16, rows: u16) -> bool {
        let id_str: String = id.into();
        if !self.sessions.contains_key(&id_str) {
            self.sessions.insert(
                id_str.clone(),
                TerminalSession::new(id_str.clone(), cols, rows),
            );
            if self.active_session_id.is_none() {
                self.active_session_id = Some(id_str);
            }
            self.generation = self.generation.wrapping_add(1);
            true
        } else {
            false
        }
    }

    /// Mark a session as being backed by xterm.js (or not).
    pub fn set_session_xterm(&mut self, id: &str, is_xterm: bool) {
        if let Some(session) = self.sessions.get_mut(id) {
            session.is_xterm = is_xterm;
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
        if let Err(e) = tauri_bridge::pty_spawn(&id_str, cwd, shell, cols, rows).await {
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
    pub async fn kill(&mut self, id: &str) {
        if let Err(e) = tauri_bridge::pty_kill(id).await {
            web_sys::console::error_1(&format!("pty_kill failed: {:?}", e).into());
        }
        self.sessions.remove(id);
        if self.active_session_id.as_deref() == Some(id) {
            self.active_session_id = self.sessions.keys().next().cloned();
        }
    }

    /// Resize a PTY session.
    pub async fn resize(&mut self, id: &str, cols: u16, rows: u16) {
        if let Some(session) = self.sessions.get_mut(id) {
            session.resize(cols, rows);
        }
        if let Err(e) = tauri_bridge::pty_resize(id, cols, rows).await {
            web_sys::console::error_1(&format!("pty_resize failed: {:?}", e).into());
        }
    }

    /// Handle incoming data from the backend.
    pub fn on_data(&mut self, id: &str, payload: &str) {
        let event: TerminalDataEvent = match serde_json::from_str(payload) {
            Ok(e) => e,
            Err(err) => {
                web_sys::console::error_1(
                    &format!("terminal:data parse error for session {}: {}", id, err).into(),
                );
                return;
            }
        };

        if let Some(session) = self.sessions.get_mut(id) {
            // For xterm-managed sessions, skip grid updates and generation bump.
            // Cursor position and visibility are still updated in case other
            // UI reads them (e.g. restore_term_from_session).
            if session.is_xterm {
                session.cursor_x = event.cursorCol;
                session.cursor_y = event.cursorRow;
                if let Some(visible) = event.cursorVisible {
                    session.cursor_visible = visible;
                }
                return;
            }

            // Resize grid if the backend reports different dimensions.
            if event.rows as u16 != session.rows || event.cols as u16 != session.cols {
                session.resize(event.cols as u16, event.rows as u16);
            }

            // Apply each cell delta to the grid.
            for delta in &event.deltas {
                let row = delta.row;
                let col = delta.col;
                if row < session.grid.len() && col < session.grid[row].len() {
                    session.grid[row][col] = TerminalCell::from_delta(delta);
                    session.dirty_rows.insert(row);
                }
            }

            // Update cursor position and visibility.
            session.cursor_x = event.cursorCol;
            session.cursor_y = event.cursorRow;
            if let Some(visible) = event.cursorVisible {
                session.cursor_visible = visible;
            }

            session.generation = session.generation.wrapping_add(1);
            session.last_update_ms = js_sys::Date::now();
        }

        self.generation = self.generation.wrapping_add(1);
    }

    /// Handle session exit from the backend.
    pub fn on_exit(&mut self, id: &str) {
        if let Some(session) = self.sessions.get_mut(id) {
            session.exited = true;
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// Update the detected foreground process + scraped task title for a pane.
    ///
    /// Called on a slow timer (the central `AgentInfoPoller`) for every pane,
    /// so it guards against spurious re-renders by only bumping `generation`
    /// when one of the values actually changed.
    pub fn update_agent_info(
        &mut self,
        id: &str,
        fg_process: Option<String>,
        task_title: Option<String>,
        session_id: Option<String>,
        raw_prompt: Option<String>,
    ) {
        if let Some(session) = self.sessions.get_mut(id) {
            if session.foreground_process != fg_process || session.task_title != task_title {
                session.foreground_process = fg_process;
                session.task_title = task_title;
                session.generation = session.generation.wrapping_add(1);
                self.generation = self.generation.wrapping_add(1);
            }
            if let Some(sid) = session_id {
                session.session_id = Some(sid);
            }
            if let Some(prompt) = raw_prompt {
                session.raw_prompt = Some(prompt);
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
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the terminal store from Dioxus context.
pub fn use_terminal_store() -> Signal<TerminalStore> {
    use_context::<Signal<TerminalStore>>()
}

/// Initialize the terminal store as a context provider.
pub fn provide_terminal_store() {
    use_context_provider(|| Signal::new(TerminalStore::default()));
}
