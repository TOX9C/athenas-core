use crate::grid::cell::Cell;
// use unicode_width::UnicodeWidthChar;

/// A row of terminal cells with occupancy tracking.
#[derive(Clone, Debug)]
pub struct Row {
    inner: Vec<Cell>,
    /// Number of occupied cells (last non-empty cell index + 1).
    pub occ: usize,
    wrapline: bool,
    dirty_start: Option<usize>,
    dirty_end: Option<usize>,
}

impl Default for Row {
    fn default() -> Self {
        Self::new(80)
    }
}

impl Row {
    pub fn new(cols: usize) -> Self {
        let mut inner = Vec::with_capacity(cols);
        for _ in 0..cols {
            inner.push(Cell::default());
        }
        Self {
            inner,
            occ: 0,
            wrapline: false,
            dirty_start: None,
            dirty_end: None,
        }
    }

    pub fn grow(&mut self, cols: usize, template: &Cell) {
        while self.inner.len() < cols {
            self.inner.push(template.clone());
        }
    }

    pub fn shrink(&mut self, cols: usize) {
        self.inner.truncate(cols);
        self.occ = self.occ.min(cols);
    }

    pub fn resize(&mut self, cols: usize, template: &Cell) {
        if cols > self.inner.len() {
            self.grow(cols, template);
        } else {
            self.shrink(cols);
        }
    }

    pub fn reset(&mut self, template: &Cell) {
        for cell in &mut self.inner {
            cell.reset(template);
        }
        self.occ = 0;
        self.wrapline = false;
        self.mark_dirty_range(0, self.inner.len());
    }

    pub fn get_cell(&self, col: usize) -> Option<&Cell> {
        self.inner.get(col)
    }

    pub fn get_cell_mut(&mut self, col: usize) -> Option<&mut Cell> {
        self.inner.get_mut(col)
    }

    pub fn set_cell(&mut self, col: usize, cell: Cell) {
        if col < self.inner.len() {
            if cell.c == '\0' || cell.c == ' ' {
                // empty-ish: might reduce occ
                if col == self.occ.saturating_sub(1) {
                    self.occ = self.inner[..col]
                        .iter()
                        .rposition(|c| !c.is_empty())
                        .map(|i| i + 1)
                        .unwrap_or(0);
                }
            } else {
                self.occ = self.occ.max(col + 1);
            }
            self.inner[col] = cell;
            self.mark_dirty(col);
        }
    }

    pub fn clear_cell(&mut self, col: usize, template: &Cell) {
        if col < self.inner.len() {
            self.inner[col].reset(template);
            if col == self.occ.saturating_sub(1) {
                self.occ = self.inner[..col]
                    .iter()
                    .rposition(|c| !c.is_empty())
                    .map(|i| i + 1)
                    .unwrap_or(0);
            }
            self.mark_dirty(col);
        }
    }

    pub fn insert_chars(&mut self, col: usize, count: usize, template: &Cell) {
        if col >= self.inner.len() {
            return;
        }
        let count = count.min(self.inner.len() - col);
        let end = self.inner.len() - count;
        // Shift right
        for i in (col..end).rev() {
            self.inner[i + count] = self.inner[i].clone();
        }
        // Insert blanks
        for i in col..(col + count) {
            self.inner[i] = template.clone();
        }
        self.occ = self
            .inner
            .iter()
            .rposition(|c| !c.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        self.mark_dirty_range(col, self.inner.len());
    }

    pub fn delete_chars(&mut self, col: usize, count: usize, template: &Cell) {
        if col >= self.inner.len() {
            return;
        }
        let count = count.min(self.inner.len() - col);
        // Shift left
        for i in (col + count)..self.inner.len() {
            self.inner[i - count] = self.inner[i].clone();
        }
        // Fill with blanks at end
        for i in (self.inner.len() - count)..self.inner.len() {
            self.inner[i] = template.clone();
        }
        self.occ = self
            .inner
            .iter()
            .rposition(|c| !c.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        self.mark_dirty_range(col, self.inner.len());
    }

    pub fn shift_right(&mut self, col: usize, template: &Cell) {
        if col + 1 < self.inner.len() {
            for i in (col..(self.inner.len() - 1)).rev() {
                self.inner[i + 1] = self.inner[i].clone();
            }
            self.inner[col] = template.clone();
            self.mark_dirty_range(col, self.inner.len());
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.occ == 0
    }

    pub fn set_wrapline(&mut self, wrap: bool) {
        self.wrapline = wrap;
    }

    pub fn wrapline(&self) -> bool {
        self.wrapline
    }

    pub fn dirty_range(&self) -> Option<(usize, usize)> {
        match (self.dirty_start, self.dirty_end) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        }
    }

    pub fn clear_dirty(&mut self) {
        self.dirty_start = None;
        self.dirty_end = None;
    }

    pub fn mark_dirty(&mut self, col: usize) {
        self.dirty_start = Some(self.dirty_start.map_or(col, |s| s.min(col)));
        self.dirty_end = Some(self.dirty_end.map_or(col, |e| e.max(col + 1)));
    }

    pub fn mark_dirty_range(&mut self, start: usize, end: usize) {
        self.dirty_start = Some(self.dirty_start.map_or(start, |s| s.min(start)));
        self.dirty_end = Some(self.dirty_end.map_or(end, |e| e.max(end)));
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Cell> {
        self.inner.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Cell> {
        self.inner.iter_mut()
    }
}
