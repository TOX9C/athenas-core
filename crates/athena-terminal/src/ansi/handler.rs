use vte::{Params, Perform};

use crate::ansi::ops::AnsiOp;

/// Collects ANSI escape sequences into a buffer of operations.
pub struct AnsiHandler {
    ops: Vec<AnsiOp>,
}

impl AnsiHandler {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn ops(self) -> Vec<AnsiOp> {
        self.ops
    }

    pub fn clear(&mut self) {
        self.ops.clear();
    }
}

impl Default for AnsiHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Perform for AnsiHandler {
    fn print(&mut self, c: char) {
        self.ops.push(AnsiOp::Print(c));
    }

    fn execute(&mut self, byte: u8) {
        self.ops.push(AnsiOp::Execute(byte));
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        let params: Vec<u16> = params
            .iter()
            .map(|p| p.first().copied().unwrap_or(0))
            .collect();
        self.ops.push(AnsiOp::Csi {
            params,
            intermediates: intermediates.to_vec(),
            ignore,
            action,
        });
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.ops.push(AnsiOp::Osc {
            params: params.iter().map(|p| p.to_vec()).collect(),
            bell_terminated,
        });
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.ops.push(AnsiOp::Esc {
            intermediates: intermediates.to_vec(),
            ignore,
            byte,
        });
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        let params: Vec<u16> = params
            .iter()
            .map(|p| p.first().copied().unwrap_or(0))
            .collect();
        self.ops.push(AnsiOp::DcsHook {
            params,
            intermediates: intermediates.to_vec(),
            ignore,
            action,
        });
    }

    fn put(&mut self, byte: u8) {
        self.ops.push(AnsiOp::DcsPut(byte));
    }

    fn unhook(&mut self) {
        self.ops.push(AnsiOp::DcsUnhook);
    }
}

/// Apply a collected list of ANSI operations to the given grid.
use crate::grid::Grid;

pub fn apply_ops(grid: &mut Grid, ops: Vec<AnsiOp>) {
    for op in ops {
        match op {
            AnsiOp::Print(c) => {
                grid.insert_char(c);
            }
            AnsiOp::Execute(byte) => {
                match byte {
                    0x07 => {} // BEL
                    0x08 => {
                        grid.move_cursor_left(1);
                        grid.delete_char();
                    } // BS: move left and erase
                    0x09 => grid.tab(), // HT
                    0x0A => grid.newline(), // LF
                    0x0B => grid.newline(), // VT
                    0x0C => grid.newline(), // FF
                    0x0D => grid.carriage_return(), // CR
                    0x85 => {
                        // NEL
                        grid.carriage_return();
                        grid.newline();
                    }
                    0x88 => {} // HTS - TODO
                    0x8D => {
                        // RI
                        let top = grid.scroll_region.top;
                        if grid.cursor.row == top {
                            grid.scroll_down(1);
                        } else {
                            grid.move_cursor_up(1);
                        }
                    }
                    _ => {}
                }
            }
            AnsiOp::Csi {
                params,
                intermediates: _,
                ignore: _,
                action,
            } => {
                apply_csi(grid, &params, action);
            }
            AnsiOp::Esc {
                intermediates: _,
                ignore: _,
                byte,
            } => {
                match byte {
                    b'7' => grid.save_cursor(),
                    b'8' => grid.restore_cursor(),
                    b'M' => {
                        let top = grid.scroll_region.top;
                        if grid.cursor.row == top {
                            grid.scroll_down(1);
                        } else {
                            grid.move_cursor_up(1);
                        }
                    }
                    b'c' => {
                        grid.clear();
                        grid.move_cursor_to(0, 0);
                    }
                    b'H' => {}        // HTS - TODO
                    b'(' | b')' => {} // Charset select - TODO
                    _ => {}
                }
            }
            AnsiOp::Osc { params, .. } => {
                if !params.is_empty() {
                    // Window title, etc.
                    let osc_str = String::from_utf8_lossy(&params[0]);
                    if let Ok(n) = osc_str.parse::<u16>() {
                        // Handle OSC sequences by number
                        match n {
                            0..=2
                                // Set window title/icon
                                if params.len() > 1 => {
                                    let title = String::from_utf8_lossy(&params[1]).to_string();
                                    grid.title = title;
                                }
                            _ => {}
                        }
                    }
                }
            }
            AnsiOp::DcsHook { .. } => {}
            AnsiOp::DcsPut(_) => {}
            AnsiOp::DcsUnhook => {}
        }
    }
}

fn apply_csi(grid: &mut Grid, params: &[u16], action: char) {
    let param_or = |idx: usize, default: u16| -> u16 {
        if let Some(&p) = params.get(idx) {
            if p == 0 {
                default
            } else {
                p
            }
        } else {
            default
        }
    };

    match action {
        'A' => grid.move_cursor_up(param_or(0, 1) as usize),
        'B' => grid.move_cursor_down(param_or(0, 1) as usize),
        'C' => grid.move_cursor_right(param_or(0, 1) as usize),
        'D' => grid.move_cursor_left(param_or(0, 1) as usize),
        'E' => {
            grid.move_cursor_down(param_or(0, 1) as usize);
            grid.move_cursor_to_col(0);
        }
        'F' => {
            grid.move_cursor_up(param_or(0, 1) as usize);
            grid.move_cursor_to_col(0);
        }
        'G' => grid.move_cursor_to_col((param_or(0, 1) as usize).saturating_sub(1)),
        'H' => {
            let row = param_or(0, 1) as usize;
            let col = param_or(1, 1) as usize;
            grid.move_cursor_to(row.saturating_sub(1), col.saturating_sub(1));
        }
        'I' => {
            let n = param_or(0, 1) as usize;
            for _ in 0..n {
                grid.tab();
            }
        }
        'J' => grid.erase_display(param_or(0, 0)),
        'K' => grid.erase_line(param_or(0, 0)),
        'S' => {
            let n = param_or(0, 1) as usize;
            for _ in 0..n {
                grid.scroll_up(1);
            }
        }
        'T' => {
            let n = param_or(0, 1) as usize;
            for _ in 0..n {
                grid.scroll_down(1);
            }
        }
        '@' => grid.insert_chars(param_or(0, 1) as usize),
        'P' => grid.delete_chars(param_or(0, 1) as usize),
        'X' => {
            let n = param_or(0, 1) as usize;
            for _ in 0..n {
                grid.delete_char();
            }
        }
        'm' => grid.set_sgr(params),
        's' => grid.save_cursor(),
        'u' => grid.restore_cursor(),
        'n' if param_or(0, 0) == 6 => {
            // DSR - Device Status Report (cursor position)
            // TODO: emit response
        }
        'd' => {
            let row = param_or(0, 1) as usize;
            grid.move_cursor_to(row.saturating_sub(1), grid.cursor.col);
        }
        'f' => {
            let row = param_or(0, 1) as usize;
            let col = param_or(1, 1) as usize;
            grid.move_cursor_to(row.saturating_sub(1), col.saturating_sub(1));
        }
        'L' => {
            // IL - Insert Line
            let n = param_or(0, 1) as usize;
            let default = grid.default_cell.clone();
            for _ in 0..n {
                if let Some(row) = grid.rows.get_mut(grid.cursor.row) {
                    row.reset(&default);
                }
            }
        }
        'M' => {
            // DL - Delete Line
            let n = param_or(0, 1) as usize;
            let default = grid.default_cell.clone();
            for _ in 0..n {
                if let Some(row) = grid.rows.get_mut(grid.cursor.row) {
                    row.reset(&default);
                }
            }
        }
        _ => {}
    }
}
