// Grid module: Cell, Row, and Grid types for the terminal.

pub mod cell;
pub mod colors;
pub mod row;

use crate::grid::cell::{Cell, CellExtra, CellFlags};
use crate::grid::colors::Color;
use crate::grid::row::Row;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Defensive cap on dirty cell deltas emitted per read cycle.
/// At ~80x24 = 1920 cells per screen, 50_000 is well above any legitimate
/// single PTY read (max 4KB buffer). If a future bug leaves the dirty bitset
/// uncleared, exceeding this escalates to a full redraw instead of emitting
/// an unbounded delta list.
pub const MAX_DIRTY_CELLS_PER_READ: usize = 50_000;

/// A Point in the grid: (row, col)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub row: usize,
    pub col: usize,
}

/// Represents a range of dirty cells for delta tracking
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellDelta {
    pub row: usize,
    pub col: usize,
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: u16,
}

/// Scroll region: top and bottom row (0-indexed, inclusive)
#[derive(Clone, Copy, Debug)]
pub struct ScrollRegion {
    pub top: usize,
    pub bottom: usize,
}

/// The terminal Grid: a fixed-width, scrollable buffer of Rows.
pub struct Grid {
    pub rows: Vec<Row>,
    scrollback: VecDeque<Row>,
    pub cursor: Point,
    saved_cursor: Option<Point>,
    pub cols: usize,
    pub rows_count: usize,
    pub scroll_region: ScrollRegion,
    _selection: Option<(Point, Point)>,
    /// Dirty-cell bitset: one bit per visible cell, row-major
    /// (`bit = row * cols + col`). Replaces the old per-cell Vec pushes so
    /// flushing needs no clone/sort/dedup.
    dirty_bits: Vec<u64>,
    /// Whole-viewport sentinel: set by scrolls, full clears, and alt-screen
    /// switches, where every visible cell changed. `dirty_deltas` emits a
    /// full redraw without any per-cell bookkeeping.
    full_redraw: bool,
    pub default_cell: Cell,
    max_scrollback: usize,
    pub cursor_visible: bool,
    pub cursor_blinking: bool,
    pub cursor_style: CursorStyle,
    pub title: String,
    pub icon_name: String,
    /// Last OSC 52 clipboard payload (decoded UTF-8), if any. The terminal
    /// crate has no clipboard sink; the session exposes this upward.
    pub osc52_clipboard: Option<String>,
    /// G0 charset: true when DEC special graphics (ESC ( 0) is selected.
    /// G0-only scope: G1 designates and SO/SI shifts are not tracked.
    charset_graphics: bool,
    /// OSC 8 hyperlink URI table; cells reference entries by id via
    /// `CellExtra::hyperlink_id`, so Cell size stays unchanged.
    hyperlinks: Vec<String>,
    /// Currently active OSC 8 hyperlink (table index), if any.
    current_hyperlink: Option<u16>,
    /// Current SGR (Select Graphic Rendition) state, applied to next cells
    current_fg: Color,
    current_bg: Color,
    current_flags: CellFlags,
    /// Track if current row should wrap to next on overflow
    wrap_next: bool,
    /// Insert/replace mode
    insert_mode: bool,
    /// Auto-wrap mode
    auto_wrap: bool,
    /// Origin mode (relative to scroll region)
    origin_mode: bool,
    /// Reverse video (screen-wide)
    _reverse_video: bool,
    /// Bracketed paste mode enabled (DECSET/DECRST 2004)
    pub bracketed_paste: bool,
    /// Alternate screen active (DECSET 1049)
    alt_screen_active: bool,
    /// Primary-screen rows and cursor saved while the alternate screen is
    /// active, restored on DECRST 1049.
    alt_saved: Option<(Vec<Row>, Point)>,
    /// Mouse tracking mode (0 = off, 1000 = normal, 1002 = motion, 1003 = all motion)
    _mouse_mode: u16,
    /// Horizontal tab stops. The default terminal layout uses stops every
    /// eight columns; HTS can add a stop at the current cursor column.
    tab_stops: Vec<bool>,
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

pub enum CursorStyle {
    Block,
    Line,
    Bar,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let mut grid = Self {
            rows: Vec::with_capacity(rows),
            scrollback: VecDeque::new(),
            cursor: Point::default(),
            saved_cursor: None,
            cols,
            rows_count: rows,
            scroll_region: ScrollRegion {
                top: 0,
                bottom: rows.saturating_sub(1),
            },
            _selection: None,
            dirty_bits: vec![0; Self::dirty_words(cols, rows)],
            full_redraw: false,
            default_cell: Cell::default(),
            max_scrollback: 10000,
            cursor_visible: true,
            cursor_blinking: true,
            cursor_style: CursorStyle::Block,
            title: String::new(),
            icon_name: String::new(),
            osc52_clipboard: None,
            charset_graphics: false,
            hyperlinks: Vec::new(),
            current_hyperlink: None,
            current_fg: Color::Named(colors::NamedColor::Foreground),
            current_bg: Color::Named(colors::NamedColor::Background),
            current_flags: CellFlags::empty(),
            wrap_next: false,
            insert_mode: false,
            auto_wrap: true,
            origin_mode: false,
            _reverse_video: false,
            bracketed_paste: false,
            alt_screen_active: false,
            alt_saved: None,
            _mouse_mode: 0,
            tab_stops: (0..cols).map(|col| col % 8 == 0).collect(),
        };
        for _ in 0..rows {
            grid.rows.push(Row::new(cols));
        }
        grid
    }

    /// Number of u64 words needed for a `cols * rows` dirty bitset.
    fn dirty_words(cols: usize, rows: usize) -> usize {
        (cols * rows).div_ceil(64)
    }

    /// Set one dirty bit. Static so callers holding a `self.rows` borrow can
    /// pass the disjoint `dirty_bits`/`cols` fields.
    #[inline]
    fn set_dirty_bit(bits: &mut [u64], cols: usize, row: usize, col: usize) {
        let idx = row * cols + col;
        // Bounds-checked: an out-of-range mark is dropped rather than
        // panicking if a cursor invariant ever drifts.
        if let Some(word) = bits.get_mut(idx >> 6) {
            *word |= 1u64 << (idx & 63);
        }
    }

    fn mark_dirty(&mut self, row: usize, col: usize) {
        Self::set_dirty_bit(&mut self.dirty_bits, self.cols, row, col);
    }

    fn mark_cols_dirty(&mut self, row: usize, cols: std::ops::Range<usize>) {
        for col in cols {
            Self::set_dirty_bit(&mut self.dirty_bits, self.cols, row, col);
        }
    }

    fn mark_row_dirty(&mut self, row: usize) {
        self.mark_cols_dirty(row, 0..self.cols);
    }

    /// Designate the G0 charset (DECSCL). Only `'0'` (DEC special graphics)
    /// and `'B'` (ASCII) are honored; G1 and SO/SI are out of scope.
    pub fn set_charset(&mut self, designator: u8) {
        self.charset_graphics = designator == b'0';
    }

    /// Translate a printable char through the active G0 charset.
    /// Unknown bytes in DEC graphics mode pass through unchanged.
    pub fn map_charset(&self, c: char) -> char {
        if !self.charset_graphics {
            return c;
        }
        dec_special_map(c)
    }

    /// Set the active OSC 8 hyperlink (empty/None clears it). Reused URIs
    /// share one table entry; table growth is bounded by distinct URIs seen.
    pub fn set_hyperlink(&mut self, uri: Option<&str>) {
        self.current_hyperlink = match uri {
            Some(u) if !u.is_empty() => {
                // ponytail: linear scan over distinct URIs; fine for realistic
                // counts, hash if a terminal ever tracks thousands of links
                let id = self
                    .hyperlinks
                    .iter()
                    .position(|s| s == u)
                    .unwrap_or_else(|| {
                        // u16 id space: on overflow, drop the link rather than
                        // misattributing it to a saturated shared id.
                        if self.hyperlinks.len() >= u16::MAX as usize {
                            self.hyperlinks.len()
                        } else {
                            self.hyperlinks.push(u.to_string());
                            self.hyperlinks.len() - 1
                        }
                    });
                u16::try_from(id).ok()
            }
            _ => None,
        };
    }

    /// Look up a hyperlink URI by the id stored on a cell.
    pub fn hyperlink_uri(&self, id: u16) -> Option<&str> {
        self.hyperlinks.get(id as usize).map(String::as_str)
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        if self.cursor.row >= self.rows_count || self.cursor.col >= self.cols {
            return;
        }
        if self.wrap_next {
            self.wrap_next = false;
            if self.cursor.row == self.scroll_region.bottom {
                // Wrap at the bottom margin scrolls the scroll region.
                self.scroll_up(1);
            } else if self.cursor.row + 1 < self.rows_count {
                self.cursor.row += 1;
            }
            self.cursor.col = 0;
            if let Some(row) = self.rows.get_mut(self.cursor.row) {
                row.set_wrapline(true);
            }
        }
        if let Some(row) = self.rows.get_mut(self.cursor.row) {
            if self.insert_mode {
                row.shift_right(self.cursor.col, &self.default_cell);
            }
            let mut cell = Cell {
                c,
                fg: self.current_fg.clone(),
                bg: self.current_bg.clone(),
                flags: self.current_flags,
                ..Default::default()
            };
            if let Some(id) = self.current_hyperlink {
                cell.extra = Some(Box::new(CellExtra {
                    hyperlink_id: Some(id),
                    ..Default::default()
                }));
            }
            row.set_cell(self.cursor.col, cell);
            Self::set_dirty_bit(&mut self.dirty_bits, self.cols, self.cursor.row, self.cursor.col);
            if self.cursor.col + 1 >= self.cols {
                if self.auto_wrap {
                    self.wrap_next = true;
                }
            } else {
                self.cursor.col += 1;
                self.wrap_next = false;
            }
        }
    }

    /// Move cursor operations
    pub fn move_cursor_left(&mut self, n: usize) {
        // DECOM constrains row addressing, never the column; always clamp at 0.
        self.cursor.col = self.cursor.col.saturating_sub(n);
        self.wrap_next = false;
    }
    pub fn move_cursor_right(&mut self, n: usize) {
        self.cursor.col = (self.cursor.col + n).min(self.cols.saturating_sub(1));
        self.wrap_next = false;
    }
    pub fn move_cursor_up(&mut self, n: usize) {
        let min_row = if self.origin_mode {
            self.scroll_region.top
        } else {
            0
        };
        self.cursor.row = self.cursor.row.saturating_sub(n).max(min_row);
    }
    pub fn move_cursor_down(&mut self, n: usize) {
        let max_row = if self.origin_mode {
            self.scroll_region.bottom
        } else {
            self.rows_count.saturating_sub(1)
        };
        self.cursor.row = (self.cursor.row + n).min(max_row);
    }
    pub fn move_cursor_to(&mut self, row: usize, col: usize) {
        self.cursor.row = row.min(self.rows_count.saturating_sub(1));
        self.cursor.col = col.min(self.cols.saturating_sub(1));
        self.wrap_next = false;
    }
    pub fn move_cursor_to_col(&mut self, col: usize) {
        self.cursor.col = col.min(self.cols.saturating_sub(1));
        self.wrap_next = false;
    }
    pub fn move_cursor_down_and_home(&mut self, n: usize) {
        self.move_cursor_down(n);
        self.cursor.col = 0;
    }
    pub fn move_cursor_up_and_home(&mut self, n: usize) {
        self.move_cursor_up(n);
        self.cursor.col = 0;
    }

    pub fn newline(&mut self) {
        self.cursor.col = 0;
        self.wrap_next = false;
        if self.cursor.row == self.scroll_region.bottom {
            // LF at the bottom margin scrolls the scroll region.
            self.scroll_up(1);
        } else if self.cursor.row + 1 < self.rows_count {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
        self.wrap_next = false;
    }

    pub fn tab(&mut self) {
        let next_tab = self
            .tab_stops
            .iter()
            .enumerate()
            .skip(self.cursor.col.saturating_add(1))
            .find_map(|(col, is_stop)| is_stop.then_some(col))
            .unwrap_or_else(|| self.cols.saturating_sub(1));
        self.cursor.col = next_tab;
        self.wrap_next = false;
    }

    /// Set a horizontal tab stop at the current cursor column (HTS).
    pub fn set_tab_stop(&mut self) {
        if let Some(stop) = self.tab_stops.get_mut(self.cursor.col) {
            *stop = true;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor.row < self.rows_count {
            let default = self.default_cell.clone();
            if let Some(row) = self.rows.get_mut(self.cursor.row) {
                row.clear_cell(self.cursor.col, &default);
                Self::set_dirty_bit(&mut self.dirty_bits, self.cols, self.cursor.row, self.cursor.col);
            }
        }
    }

    /// Scroll the visible region up by `lines` rows
    pub fn scroll_up(&mut self, lines: usize) {
        for _ in 0..lines {
            let removed = self.rows.remove(self.scroll_region.top);
            // Only full-screen scrolling feeds scrollback; lines rotated out
            // of a DECSTB region are discarded (xterm semantics), otherwise
            // TUIs like tmux pollute the history with region churn.
            if self.scroll_region.top == 0 {
                if self.scrollback.len() >= self.max_scrollback {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(removed);
            }
            self.rows
                .insert(self.scroll_region.bottom, Row::new(self.cols));
        }
        // Every visible cell in the region moved; a full redraw needs no
        // per-cell bookkeeping.
        self.full_redraw = true;
    }

    /// Scroll the visible region down by `lines` rows
    pub fn scroll_down(&mut self, lines: usize) {
        for _ in 0..lines {
            self.rows.remove(self.scroll_region.bottom);
            self.rows
                .insert(self.scroll_region.top, Row::new(self.cols));
        }
        self.full_redraw = true;
    }

    /// Erase in display (ED): 0=cursor to end, 1=start to cursor, 2=whole, 3=scrollback
    pub fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                // Erase from cursor to end of display
                let (crow, ccol) = (self.cursor.row, self.cursor.col);
                if let Some(row) = self.rows.get_mut(crow) {
                    for col in ccol..self.cols {
                        row.clear_cell(col, &self.default_cell);
                    }
                }
                for row in (crow + 1)..self.rows_count {
                    if let Some(r) = self.rows.get_mut(row) {
                        r.reset(&self.default_cell);
                    }
                }
                self.mark_cols_dirty(crow, ccol..self.cols);
                for row in (crow + 1)..self.rows_count {
                    self.mark_row_dirty(row);
                }
            }
            1 => {
                // Erase from start to cursor
                let (crow, ccol) = (self.cursor.row, self.cursor.col);
                for row in 0..crow {
                    if let Some(r) = self.rows.get_mut(row) {
                        r.reset(&self.default_cell);
                    }
                }
                if let Some(row) = self.rows.get_mut(crow) {
                    for col in 0..=ccol {
                        row.clear_cell(col, &self.default_cell);
                    }
                }
                for row in 0..crow {
                    self.mark_row_dirty(row);
                }
                self.mark_cols_dirty(crow, 0..ccol + 1);
            }
            2 => {
                // Erase entire display
                for row in 0..self.rows_count {
                    if let Some(r) = self.rows.get_mut(row) {
                        r.reset(&self.default_cell);
                    }
                }
                self.full_redraw = true;
            }
            3 => {
                // Erase scrollback buffer
                self.scrollback.clear();
            }
            _ => {}
        }
    }

    /// Erase in line (EL): 0=cursor to end, 1=start to cursor, 2=whole
    pub fn erase_line(&mut self, mode: u16) {
        if self.cursor.row >= self.rows_count {
            return;
        }
        match mode {
            0 => {
                let (crow, ccol) = (self.cursor.row, self.cursor.col);
                if let Some(row) = self.rows.get_mut(crow) {
                    for col in ccol..self.cols {
                        row.clear_cell(col, &self.default_cell);
                    }
                }
                self.mark_cols_dirty(crow, ccol..self.cols);
            }
            1 => {
                let (crow, ccol) = (self.cursor.row, self.cursor.col);
                if let Some(row) = self.rows.get_mut(crow) {
                    for col in 0..=ccol {
                        row.clear_cell(col, &self.default_cell);
                    }
                }
                self.mark_cols_dirty(crow, 0..ccol + 1);
            }
            2 => {
                if let Some(row) = self.rows.get_mut(self.cursor.row) {
                    row.reset(&self.default_cell);
                }
                let crow = self.cursor.row;
                self.mark_row_dirty(crow);
            }
            _ => {}
        }
    }

    /// Set SGR (Select Graphic Rendition) from parameter list
    pub fn set_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.current_fg = Color::Named(colors::NamedColor::Foreground);
            self.current_bg = Color::Named(colors::NamedColor::Background);
            self.current_flags = CellFlags::empty();
            return;
        }
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => {
                    self.current_fg = Color::Named(colors::NamedColor::Foreground);
                    self.current_bg = Color::Named(colors::NamedColor::Background);
                    self.current_flags = CellFlags::empty();
                }
                1 => self.current_flags |= CellFlags::BOLD,
                3 => self.current_flags |= CellFlags::ITALIC,
                4 => self.current_flags |= CellFlags::UNDERLINE,
                5 => self.current_flags |= CellFlags::BLINK,
                7 => self.current_flags |= CellFlags::INVERSE,
                8 => self.current_flags |= CellFlags::INVISIBLE,
                9 => self.current_flags |= CellFlags::STRIKEOUT,
                21 => self.current_flags -= CellFlags::BOLD,
                22 => self.current_flags -= CellFlags::BOLD,
                23 => self.current_flags -= CellFlags::ITALIC,
                24 => self.current_flags -= CellFlags::UNDERLINE,
                25 => self.current_flags -= CellFlags::BLINK,
                27 => self.current_flags -= CellFlags::INVERSE,
                28 => self.current_flags -= CellFlags::INVISIBLE,
                29 => self.current_flags -= CellFlags::STRIKEOUT,
                30..=37 => {
                    self.current_fg =
                        Color::Named(colors::NamedColor::from_ansi(params[i] as u8 - 30));
                }
                38 => {
                    i += 1;
                    if i < params.len() && params[i] == 2 && i + 3 < params.len() {
                        self.current_fg = Color::Rgb(
                            params[i + 1] as u8,
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                        );
                        i += 3;
                    } else if i < params.len() && params[i] == 5 && i + 1 < params.len() {
                        self.current_fg = Color::Indexed(params[i + 1] as u8);
                        i += 1;
                    } else {
                        // Truncated/ malformed extended-color sequence. Consume
                        // whatever params remain so the trailing values are
                        // NOT re-interpreted as fresh SGR codes (e.g. a
                        // truncated `38;2;10` must not treat the `10` as a new
                        // SGR selector). Best-effort: skip to the end.
                        i = params.len();
                    }
                }
                39 => self.current_fg = Color::Named(colors::NamedColor::Foreground),
                40..=47 => {
                    self.current_bg =
                        Color::Named(colors::NamedColor::from_ansi(params[i] as u8 - 40));
                }
                48 => {
                    i += 1;
                    if i < params.len() && params[i] == 2 && i + 3 < params.len() {
                        self.current_bg = Color::Rgb(
                            params[i + 1] as u8,
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                        );
                        i += 3;
                    } else if i < params.len() && params[i] == 5 && i + 1 < params.len() {
                        self.current_bg = Color::Indexed(params[i + 1] as u8);
                        i += 1;
                    } else {
                        // Truncated/malformed: skip to end (see 38 above).
                        i = params.len();
                    }
                }
                49 => self.current_bg = Color::Named(colors::NamedColor::Background),
                90..=97 => {
                    self.current_fg =
                        Color::Named(colors::NamedColor::from_ansi(params[i] as u8 - 82));
                }
                100..=107 => {
                    self.current_bg =
                        Color::Named(colors::NamedColor::from_ansi(params[i] as u8 - 92));
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Save cursor position
    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some(self.cursor);
    }

    /// Restore cursor position
    pub fn restore_cursor(&mut self) {
        if let Some(saved) = self.saved_cursor {
            self.cursor = saved;
        }
    }

    /// Set scroll region
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let top = top.min(self.rows_count.saturating_sub(1));
        let bottom = bottom.max(top).min(self.rows_count.saturating_sub(1));
        self.scroll_region = ScrollRegion { top, bottom };
    }

    /// Resize the grid
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols == self.cols && rows == self.rows_count {
            return;
        }
        // Simple resize: truncate or pad rows
        self.rows.truncate(rows);
        while self.rows.len() < rows {
            self.rows.push(Row::new(cols));
        }
        for row in &mut self.rows {
            row.resize(cols, &self.default_cell);
        }
        self.tab_stops.resize(cols, false);
        for col in 0..cols {
            if col % 8 == 0 && self.tab_stops.get(col).is_some() {
                // Preserve the conventional default stops for newly-created
                // columns without overwriting explicit stops in existing ones.
                if col >= self.cols {
                    self.tab_stops[col] = true;
                }
            }
        }
        self.cols = cols;
        self.rows_count = rows;
        // Layout meaningfully changed: any pending per-cell marks refer to
        // the old dimensions, so rebuild the bitset and demand a full redraw.
        self.dirty_bits = vec![0; Self::dirty_words(cols, rows)];
        self.full_redraw = true;
        // Keep the scroll region in range: a shrink must never leave `top`
        // past the last row (scroll_up indexes `rows` by it).
        let max_row = rows.saturating_sub(1);
        self.scroll_region.top = self.scroll_region.top.min(max_row);
        self.scroll_region.bottom = self
            .scroll_region
            .bottom
            .min(max_row)
            .max(self.scroll_region.top);
        if self.cursor.row >= rows {
            self.cursor.row = rows.saturating_sub(1);
        }
        if self.cursor.col >= cols {
            self.cursor.col = cols.saturating_sub(1);
        }
    }

    /// Get all dirty cells as deltas, in row-major order.
    ///
    /// Backed by a bitset, so dedup is free — but if the set-bit count still
    /// exceeds `MAX_DIRTY_CELLS_PER_READ` the truncated remainder would be
    /// silently lost (`clear_dirty()` runs right after this call), so we
    /// escalate to a full redraw of every visible cell — conservative, but
    /// no frame content is dropped. Whole-viewport changes (scrolls, clears,
    /// alt-screen switches) set `full_redraw` directly and take the same
    /// escalation path without any per-cell bookkeeping.
    pub fn dirty_deltas(&self) -> Vec<CellDelta> {
        let mut full = self.full_redraw;
        if !full {
            let count: usize = self
                .dirty_bits
                .iter()
                .map(|w| w.count_ones() as usize)
                .sum();
            if count > MAX_DIRTY_CELLS_PER_READ {
                log::warn!(
                    "dirty bitset has {} cells set (> {}); falling back to full redraw",
                    count,
                    MAX_DIRTY_CELLS_PER_READ,
                );
                full = true;
            }
        }

        if full {
            let mut deltas = Vec::with_capacity(self.rows_count * self.cols);
            for row in 0..self.rows_count {
                for col in 0..self.cols {
                    if let Some(cell) = self.rows.get(row).and_then(|r| r.get_cell(col)) {
                        deltas.push(CellDelta {
                            row,
                            col,
                            c: cell.c,
                            fg: cell.fg.clone(),
                            bg: cell.bg.clone(),
                            flags: cell.flags.bits(),
                        });
                    }
                }
            }
            return deltas;
        }

        let mut deltas = Vec::new();
        for (word_idx, word) in self.dirty_bits.iter().enumerate() {
            let mut w = *word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                w &= w - 1;
                let idx = word_idx * 64 + bit;
                let (row, col) = (idx / self.cols, idx % self.cols);
                // Tail bits past the grid area are never set, but skip
                // defensively if dimensions shrank without a rebuild.
                if row >= self.rows_count {
                    continue;
                }
                if let Some(cell) = self.rows.get(row).and_then(|r| r.get_cell(col)) {
                    deltas.push(CellDelta {
                        row,
                        col,
                        c: cell.c,
                        fg: cell.fg.clone(),
                        bg: cell.bg.clone(),
                        flags: cell.flags.bits(),
                    });
                }
            }
        }
        deltas
    }

    /// Clear dirty tracking
    pub fn clear_dirty(&mut self) {
        self.dirty_bits.fill(0);
        self.full_redraw = false;
    }

    /// Enter the alternate screen (DECSET 1049): save the primary screen and
    /// cursor, switch to a blank buffer, home the cursor.
    pub fn enter_alt_screen(&mut self) {
        if self.alt_screen_active {
            return;
        }
        let saved_cursor = self.cursor;
        let blank = (0..self.rows_count)
            .map(|_| Row::new(self.cols))
            .collect::<Vec<_>>();
        let old = std::mem::replace(&mut self.rows, blank);
        self.alt_saved = Some((old, saved_cursor));
        self.alt_screen_active = true;
        self.cursor = Point::default();
        self.wrap_next = false;
        self.full_redraw = true;
    }

    /// Leave the alternate screen (DECRST 1049), restoring the primary screen
    /// and cursor position saved on entry.
    pub fn exit_alt_screen(&mut self) {
        if !self.alt_screen_active {
            return;
        }
        if let Some((mut saved_rows, saved_cursor)) = self.alt_saved.take() {
            // The grid may have been resized while the alternate screen was
            // active; bring the saved buffer to the current dimensions.
            saved_rows.truncate(self.rows_count);
            while saved_rows.len() < self.rows_count {
                saved_rows.push(Row::new(self.cols));
            }
            for row in &mut saved_rows {
                row.resize(self.cols, &self.default_cell);
            }
            self.rows = saved_rows;
            self.cursor = Point {
                row: saved_cursor.row.min(self.rows_count.saturating_sub(1)),
                col: saved_cursor.col.min(self.cols.saturating_sub(1)),
            };
        }
        self.alt_screen_active = false;
        self.wrap_next = false;
        self.full_redraw = true;
    }

    /// Insert `n` blank lines at the cursor row (IL). Lines shift down within
    /// the scroll region; lines pushed past the bottom margin are discarded.
    /// No-op when the cursor is outside the scroll region.
    pub fn insert_lines(&mut self, n: usize) {
        let cursor_row = self.cursor.row;
        let region = self.scroll_region;
        if cursor_row < region.top || cursor_row > region.bottom {
            return;
        }
        for _ in 0..n {
            self.rows.insert(cursor_row, Row::new(self.cols));
            // The insertion shifted the bottom margin down by one.
            self.rows.remove(region.bottom + 1);
        }
        for row in cursor_row..=region.bottom {
            self.mark_row_dirty(row);
        }
    }

    /// Delete `n` lines at the cursor row (DL). Lines below shift up within
    /// the scroll region; blank lines are pulled in at the bottom margin.
    /// No-op when the cursor is outside the scroll region.
    pub fn delete_lines(&mut self, n: usize) {
        let cursor_row = self.cursor.row;
        let region = self.scroll_region;
        if cursor_row < region.top || cursor_row > region.bottom {
            return;
        }
        for _ in 0..n {
            self.rows.remove(cursor_row);
            self.rows.insert(region.bottom, Row::new(self.cols));
        }
        for row in cursor_row..=region.bottom {
            self.mark_row_dirty(row);
        }
    }

    /// Insert blank characters at cursor (ICH)
    pub fn insert_chars(&mut self, count: usize) {
        let (crow, ccol) = (self.cursor.row, self.cursor.col);
        if let Some(row) = self.rows.get_mut(crow) {
            row.insert_chars(ccol, count, &self.default_cell);
        }
        self.mark_cols_dirty(crow, ccol..self.cols);
    }

    /// Delete characters at cursor (DCH)
    pub fn delete_chars(&mut self, count: usize) {
        let (crow, ccol) = (self.cursor.row, self.cursor.col);
        if let Some(row) = self.rows.get_mut(crow) {
            row.delete_chars(ccol, count, &self.default_cell);
        }
        self.mark_cols_dirty(crow, ccol..self.cols);
    }

    /// Get cell at position
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.rows.get(row).and_then(|r| r.get_cell(col))
    }

    /// Get mutable cell at position
    pub fn get_cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.rows.get_mut(row).and_then(|r| {
            r.mark_dirty(col);
            r.get_cell_mut(col)
        })
    }

    /// Get row reference
    pub fn get_row(&self, row: usize) -> Option<&Row> {
        self.rows.get(row)
    }

    /// Get row mutable
    pub fn get_row_mut(&mut self, row: usize) -> Option<&mut Row> {
        self.rows.get_mut(row)
    }

    /// Set cell directly
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) {
        if let Some(r) = self.rows.get_mut(row) {
            r.set_cell(col, cell);
        }
        self.mark_dirty(row, col);
    }

    /// Get current scrollback rows
    pub fn scrollback_rows(&self) -> &VecDeque<Row> {
        &self.scrollback
    }

    /// Clear entire grid
    pub fn clear(&mut self) {
        for row in &mut self.rows {
            row.reset(&self.default_cell);
        }
        self.full_redraw = true;
    }
}

/// VT100 DEC special-graphics charset (ESC ( 0). Chars not in this table
/// pass through unchanged.
pub fn dec_special_map(c: char) -> char {
    match c {
        '_' => ' ',
        '`' => '◆',
        'a' => '▒',
        'f' => '°',
        'g' => '±',
        'h' => '␤',
        'i' => '␛',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_new_has_rows() {
        let g = Grid::new(80, 24);
        assert_eq!(g.rows.len(), 24);
        assert_eq!(g.cols, 80);
    }

    #[test]
    fn grid_insert_char_moves_cursor() {
        let mut g = Grid::new(80, 24);
        g.insert_char('H');
        assert_eq!(g.cursor.col, 1);
        assert_eq!(g.get_cell(0, 0).map(|c| c.c), Some('H'));
    }

    #[test]
    fn grid_newline() {
        let mut g = Grid::new(80, 24);
        g.insert_char('A');
        g.newline();
        assert_eq!(g.cursor.row, 1);
        assert_eq!(g.cursor.col, 0);
    }

    #[test]
    fn grid_scroll_up() {
        let mut g = Grid::new(80, 24);
        g.insert_char('A');
        g.newline();
        g.insert_char('B');
        // Fill to bottom, then force scroll
        for _ in 0..22 {
            g.newline();
        }
        g.insert_char('Z');
        // Now scroll up should push rows into scrollback
        g.scroll_up(1);
        assert_eq!(g.scrollback.len(), 1);
    }

    #[test]
    fn grid_erase_line() {
        let mut g = Grid::new(80, 24);
        g.insert_char('A');
        g.insert_char('B');
        g.move_cursor_to(0, 0);
        g.erase_line(2); // Erase whole line
        assert_eq!(g.get_cell(0, 0).map(|c| c.c), Some('\0'));
        assert_eq!(g.get_cell(0, 1).map(|c| c.c), Some('\0'));
    }

    #[test]
    fn grid_sgr_bold() {
        let mut g = Grid::new(80, 24);
        g.set_sgr(&[1]);
        assert!(g.current_flags.contains(CellFlags::BOLD));
    }

    #[test]
    fn tab_stops_can_be_set_and_used() {
        let mut g = Grid::new(20, 4);
        g.move_cursor_to_col(2);
        g.set_tab_stop();
        g.move_cursor_to_col(0);
        g.tab();
        assert_eq!(g.cursor.col, 2);
    }

    #[test]
    fn resize_clamps_scroll_region_after_shrink() {
        let mut g = Grid::new(80, 24);
        g.set_scroll_region(20, 23);
        g.resize(80, 10);
        assert_eq!(g.scroll_region.top, 9);
        assert_eq!(g.scroll_region.bottom, 9);
        // Must not panic: top is back in range.
        g.scroll_up(1);
        assert_eq!(g.rows.len(), 10);
    }

    #[test]
    fn newline_scrolls_at_region_bottom() {
        let mut g = Grid::new(80, 10);
        g.set_scroll_region(2, 5);
        g.move_cursor_to(5, 0);
        g.newline();
        // Scrolled the region; cursor stays at the bottom margin.
        assert_eq!(g.cursor.row, 5);
        // Region scroll must not feed scrollback.
        assert_eq!(g.scrollback.len(), 0);
    }

    #[test]
    fn newline_below_region_advances_normally() {
        let mut g = Grid::new(80, 10);
        g.set_scroll_region(0, 4);
        g.move_cursor_to(6, 3);
        g.newline();
        assert_eq!(g.cursor.row, 7);
    }

    #[test]
    fn full_region_scroll_still_feeds_scrollback() {
        let mut g = Grid::new(80, 4);
        g.scroll_up(1);
        assert_eq!(g.scrollback.len(), 1);
    }

    #[test]
    fn alt_screen_save_restore() {
        let mut g = Grid::new(10, 4);
        g.insert_char('A');
        g.move_cursor_to(2, 3);
        g.enter_alt_screen();
        assert_eq!(g.cursor, Point { row: 0, col: 0 });
        assert_eq!(g.get_cell(0, 0).map(|c| c.c), Some('\0'));
        g.exit_alt_screen();
        assert_eq!(g.cursor, Point { row: 2, col: 3 });
        assert_eq!(g.get_cell(0, 0).map(|c| c.c), Some('A'));
    }

    #[test]
    fn insert_and_delete_lines_shift_within_region() {
        let mut g = Grid::new(5, 4);
        for (i, ch) in "abcd".chars().enumerate() {
            g.move_cursor_to(i, 0);
            g.insert_char(ch);
        }
        // IL: insert one blank line at row 1
        g.move_cursor_to(1, 0);
        g.insert_lines(1);
        assert_eq!(g.get_cell(1, 0).map(|c| c.c), Some('\0'));
        assert_eq!(g.get_cell(2, 0).map(|c| c.c), Some('b'));
        assert_eq!(g.get_cell(3, 0).map(|c| c.c), Some('c'));
        // DL: remove the blank line again ('d' was discarded past the
        // bottom margin by the IL, so the last row stays blank).
        g.delete_lines(1);
        assert_eq!(g.get_cell(1, 0).map(|c| c.c), Some('b'));
        assert_eq!(g.get_cell(3, 0).map(|c| c.c), Some('\0'));
    }

    #[test]
    fn dirty_deltas_reports_single_cell_and_clears() {
        let mut g = Grid::new(80, 24);
        g.set_cell(3, 5, Cell::default());
        let deltas = g.dirty_deltas();
        assert_eq!(deltas.len(), 1);
        assert_eq!((deltas[0].row, deltas[0].col), (3, 5));
        g.clear_dirty();
        assert!(g.dirty_deltas().is_empty());
    }

    #[test]
    fn dirty_deltas_dedups_repeated_marks() {
        let mut g = Grid::new(80, 24);
        g.set_cell(1, 1, Cell::default());
        g.insert_char('x');
        g.insert_char('x');
        let mut seen = g
            .dirty_deltas()
            .into_iter()
            .map(|d| (d.row, d.col))
            .collect::<Vec<_>>();
        assert!(
            seen.windows(2).all(|w| w[0] < w[1]),
            "deltas sorted+unique: {seen:?}"
        );
        seen.dedup();
        assert_eq!(seen.len(), 3); // (1,1) + two insert cells
    }

    #[test]
    fn scroll_emits_full_viewport_redraw() {
        let mut g = Grid::new(80, 24);
        g.set_cell(0, 0, Cell::default());
        g.clear_dirty();
        g.scroll_up(1);
        assert_eq!(
            g.dirty_deltas().len(),
            80 * 24,
            "scroll must escalate to full redraw"
        );
        g.clear_dirty();
        assert!(g.dirty_deltas().is_empty());
    }

    #[test]
    fn erase_line_marks_only_that_row() {
        let mut g = Grid::new(80, 24);
        g.insert_char('A');
        g.erase_line(2);
        let deltas = g.dirty_deltas();
        assert!(deltas.iter().all(|d| d.row == 0));
        assert_eq!(deltas.len(), 80);
    }

    #[test]
    fn resize_rebuilds_bitset_full_redraw() {
        let mut g = Grid::new(80, 24);
        g.clear_dirty();
        g.resize(100, 30);
        assert_eq!(g.dirty_deltas().len(), 100 * 30);
        g.clear_dirty();
        g.set_cell(29, 99, Cell::default());
        assert_eq!(g.dirty_deltas().len(), 1);
    }
}
