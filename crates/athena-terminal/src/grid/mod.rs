// Grid module: Cell, Row, and Grid types for the terminal.

pub mod cell;
pub mod colors;
pub mod row;

use crate::grid::cell::{Cell, CellFlags};
use crate::grid::colors::Color;
use crate::grid::row::Row;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Defensive cap on dirty cell deltas emitted per read cycle.
/// At ~80x24 = 1920 cells per screen, 50_000 is well above any legitimate
/// single PTY read (max 4KB buffer), preventing unbounded growth if a future
/// bug causes dirty_cells to accumulate without being cleared.
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
    selection: Option<(Point, Point)>,
    dirty_cells: Vec<(usize, usize)>,
    pub default_cell: Cell,
    max_scrollback: usize,
    pub cursor_visible: bool,
    pub cursor_blinking: bool,
    pub cursor_style: CursorStyle,
    pub title: String,
    pub icon_name: String,
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
    reverse_video: bool,
    /// Bracketed paste mode enabled
    bracketed_paste: bool,
    /// Mouse tracking mode (0 = off, 1000 = normal, 1002 = motion, 1003 = all motion)
    mouse_mode: u16,
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
            selection: None,
            dirty_cells: Vec::new(),
            default_cell: Cell::default(),
            max_scrollback: 10000,
            cursor_visible: true,
            cursor_blinking: true,
            cursor_style: CursorStyle::Block,
            title: String::new(),
            icon_name: String::new(),
            current_fg: Color::Named(colors::NamedColor::Foreground),
            current_bg: Color::Named(colors::NamedColor::Background),
            current_flags: CellFlags::empty(),
            wrap_next: false,
            insert_mode: false,
            auto_wrap: true,
            origin_mode: false,
            reverse_video: false,
            bracketed_paste: false,
            mouse_mode: 0,
        };
        for _ in 0..rows {
            grid.rows.push(Row::new(cols));
        }
        grid
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        if self.cursor.row >= self.rows_count || self.cursor.col >= self.cols {
            return;
        }
        if self.wrap_next {
            self.wrap_next = false;
            if self.cursor.row + 1 >= self.rows_count {
                self.scroll_up(1);
                self.cursor.row = self.rows_count.saturating_sub(1);
            } else {
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
            let cell = Cell {
                c,
                fg: self.current_fg.clone(),
                bg: self.current_bg.clone(),
                flags: self.current_flags,
                ..Default::default()
            };
            row.set_cell(self.cursor.col, cell);
            self.dirty_cells.push((self.cursor.row, self.cursor.col));
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
        let min_col = if self.origin_mode {
            self.scroll_region.top
        } else {
            0
        };
        self.cursor.col = self.cursor.col.saturating_sub(n).max(min_col);
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
        if self.cursor.row + 1 >= self.rows_count {
            self.scroll_up(1);
        } else {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
        self.wrap_next = false;
    }

    pub fn tab(&mut self) {
        let next_tab = (self.cursor.col / 8 + 1) * 8;
        self.cursor.col = next_tab.min(self.cols.saturating_sub(1));
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
                self.dirty_cells.push((self.cursor.row, self.cursor.col));
            }
        }
    }

    /// Scroll the visible region up by `lines` rows
    pub fn scroll_up(&mut self, lines: usize) {
        for _ in 0..lines {
            let removed = self.rows.remove(self.scroll_region.top);
            if self.scrollback.len() >= self.max_scrollback {
                self.scrollback.pop_front();
            }
            self.scrollback.push_back(removed);
            self.rows
                .insert(self.scroll_region.bottom, Row::new(self.cols));
        }
        // Mark all cells in scroll region as dirty
        for row in self.scroll_region.top..=self.scroll_region.bottom {
            for col in 0..self.cols {
                self.dirty_cells.push((row, col));
            }
        }
    }

    /// Scroll the visible region down by `lines` rows
    pub fn scroll_down(&mut self, lines: usize) {
        for _ in 0..lines {
            self.rows.remove(self.scroll_region.bottom);
            self.rows
                .insert(self.scroll_region.top, Row::new(self.cols));
        }
        for row in self.scroll_region.top..=self.scroll_region.bottom {
            for col in 0..self.cols {
                self.dirty_cells.push((row, col));
            }
        }
    }

    /// Erase in display (ED): 0=cursor to end, 1=start to cursor, 2=whole, 3=scrollback
    pub fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                // Erase from cursor to end of display
                if let Some(row) = self.rows.get_mut(self.cursor.row) {
                    for col in self.cursor.col..self.cols {
                        row.clear_cell(col, &self.default_cell);
                        self.dirty_cells.push((self.cursor.row, col));
                    }
                }
                for row in (self.cursor.row + 1)..self.rows_count {
                    if let Some(r) = self.rows.get_mut(row) {
                        r.reset(&self.default_cell);
                    }
                    for col in 0..self.cols {
                        self.dirty_cells.push((row, col));
                    }
                }
            }
            1 => {
                // Erase from start to cursor
                for row in 0..self.cursor.row {
                    if let Some(r) = self.rows.get_mut(row) {
                        r.reset(&self.default_cell);
                    }
                    for col in 0..self.cols {
                        self.dirty_cells.push((row, col));
                    }
                }
                if let Some(row) = self.rows.get_mut(self.cursor.row) {
                    for col in 0..=self.cursor.col {
                        row.clear_cell(col, &self.default_cell);
                        self.dirty_cells.push((self.cursor.row, col));
                    }
                }
            }
            2 => {
                // Erase entire display
                for row in 0..self.rows_count {
                    if let Some(r) = self.rows.get_mut(row) {
                        r.reset(&self.default_cell);
                    }
                    for col in 0..self.cols {
                        self.dirty_cells.push((row, col));
                    }
                }
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
                if let Some(row) = self.rows.get_mut(self.cursor.row) {
                    for col in self.cursor.col..self.cols {
                        row.clear_cell(col, &self.default_cell);
                        self.dirty_cells.push((self.cursor.row, col));
                    }
                }
            }
            1 => {
                if let Some(row) = self.rows.get_mut(self.cursor.row) {
                    for col in 0..=self.cursor.col {
                        row.clear_cell(col, &self.default_cell);
                        self.dirty_cells.push((self.cursor.row, col));
                    }
                }
            }
            2 => {
                if let Some(row) = self.rows.get_mut(self.cursor.row) {
                    row.reset(&self.default_cell);
                    for col in 0..self.cols {
                        self.dirty_cells.push((self.cursor.row, col));
                    }
                }
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
        self.cols = cols;
        self.rows_count = rows;
        self.scroll_region.bottom = self.scroll_region.bottom.min(rows.saturating_sub(1));
        if self.cursor.row >= rows {
            self.cursor.row = rows.saturating_sub(1);
        }
        if self.cursor.col >= cols {
            self.cursor.col = cols.saturating_sub(1);
        }
    }

    /// Get all dirty cells as deltas. If dirty_cells exceeds
    /// `MAX_DIRTY_CELLS_PER_READ`, truncate with a warning — this is a safety
    /// net against runaway accumulation. The next read will catch the rest.
    ///
    /// Duplicates are removed: a single `erase_display(2)`/`clear()` can push
    /// `rows*cols` entries, and repeated clears within one parse batch push
    /// that many *again*. Without dedup the frontend would re-render the same
    /// cell N times per cycle. We sort+dedup the (row, col) pairs in a local
    /// buffer (the field is `&self`, so we can't mutate it in place).
    pub fn dirty_deltas(&self) -> Vec<CellDelta> {
        let truncated = self.dirty_cells.len() > MAX_DIRTY_CELLS_PER_READ;
        if truncated {
            log::warn!(
                "dirty_cells exceeded {} entries ({}), truncating this cycle",
                MAX_DIRTY_CELLS_PER_READ,
                self.dirty_cells.len()
            );
        }
        let mut unique: Vec<(usize, usize)> = self
            .dirty_cells
            .iter()
            .copied()
            .take(MAX_DIRTY_CELLS_PER_READ)
            .collect();
        unique.sort_unstable();
        unique.dedup();

        let mut deltas = Vec::with_capacity(unique.len());
        for (row, col) in unique {
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
        deltas
    }

    /// Clear dirty tracking
    pub fn clear_dirty(&mut self) {
        self.dirty_cells.clear();
    }

    /// Insert blank characters at cursor (ICH)
    pub fn insert_chars(&mut self, count: usize) {
        if let Some(row) = self.rows.get_mut(self.cursor.row) {
            row.insert_chars(self.cursor.col, count, &self.default_cell);
            for col in self.cursor.col..self.cols {
                self.dirty_cells.push((self.cursor.row, col));
            }
        }
    }

    /// Delete characters at cursor (DCH)
    pub fn delete_chars(&mut self, count: usize) {
        if let Some(row) = self.rows.get_mut(self.cursor.row) {
            row.delete_chars(self.cursor.col, count, &self.default_cell);
            for col in self.cursor.col..self.cols {
                self.dirty_cells.push((self.cursor.row, col));
            }
        }
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
            self.dirty_cells.push((row, col));
        }
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
        for row in 0..self.rows_count {
            for col in 0..self.cols {
                self.dirty_cells.push((row, col));
            }
        }
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
}
