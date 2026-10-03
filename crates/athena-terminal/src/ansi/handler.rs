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

/// Apply parsed operations to the grid, discarding protocol responses.
///
/// Kept as the stable convenience API for callers that only need rendering.
pub fn apply_ops(grid: &mut Grid, ops: Vec<AnsiOp>) {
    let _ = apply_ops_with_responses(grid, ops);
}

/// Apply parsed operations and return terminal-protocol responses that must be
/// written back to the PTY by the session owner.
pub fn apply_ops_with_responses(grid: &mut Grid, ops: Vec<AnsiOp>) -> Vec<Vec<u8>> {
    let mut responses = Vec::new();
    for op in ops {
        match op {
            AnsiOp::Print(c) => {
                grid.insert_char(c);
            }
            AnsiOp::Execute(byte) => {
                match byte {
                    0x07 => {}                        // BEL
                    0x08 => grid.move_cursor_left(1), // BS: non-destructive (ECMA-48)
                    0x09 => grid.tab(),               // HT
                    0x0A => grid.newline(),           // LF
                    0x0B => grid.newline(),           // VT
                    0x0C => grid.newline(),           // FF
                    0x0D => grid.carriage_return(),   // CR
                    0x85 => {
                        // NEL
                        grid.carriage_return();
                        grid.newline();
                    }
                    0x88 => grid.set_tab_stop(), // HTS - set tab stop
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
                intermediates,
                ignore: _,
                action,
            } => {
                if let Some(response) = apply_csi(grid, &params, &intermediates, action) {
                    responses.push(response);
                }
            }
            AnsiOp::Esc {
                intermediates,
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
                    b'H' if intermediates.is_empty() => grid.set_tab_stop(), // HTS
                    // ESC ( B / ESC ) 0 select a character set. The grid
                    // currently renders Unicode directly and has no alternate
                    // charset state, so these selectors are safely consumed.
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
    responses
}

fn apply_csi(
    grid: &mut Grid,
    params: &[u16],
    intermediates: &[u8],
    action: char,
) -> Option<Vec<u8>> {
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

    let mut response = None;
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
        'n' if intermediates.is_empty() && param_or(0, 0) == 6 => {
            // DSR 6: report cursor position using the standard 1-based
            // `CSI row ; col R` response. The caller writes this response to
            // the PTY after releasing the grid lock.
            response =
                Some(format!("\x1b[{};{}R", grid.cursor.row + 1, grid.cursor.col + 1).into_bytes());
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
            // IL - Insert Line: shift lines down within the scroll region.
            grid.insert_lines(param_or(0, 1) as usize);
        }
        'M' => {
            // DL - Delete Line: shift lines up within the scroll region.
            grid.delete_lines(param_or(0, 1) as usize);
        }
        'r' if intermediates.is_empty() => {
            // DECSTB: set top/bottom margins (1-based). Defaults: full screen.
            let top = param_or(0, 1) as usize;
            let bottom = param_or(1, grid.rows_count as u16) as usize;
            grid.set_scroll_region(top.saturating_sub(1), bottom.saturating_sub(1));
            // DECSTB homes the cursor.
            grid.move_cursor_to(0, 0);
        }
        'h' if intermediates == b"?" => apply_private_mode(grid, params, true),
        'l' if intermediates == b"?" => apply_private_mode(grid, params, false),
        _ => {}
    }
    response
}

/// Apply DEC private mode set/reset (`CSI ? Pm h` / `CSI ? Pm l`).
fn apply_private_mode(grid: &mut Grid, params: &[u16], enable: bool) {
    for &mode in params {
        match mode {
            25 => grid.cursor_visible = enable,
            // Alternate screen family (xterm): 47 = alt screen, 1047 = alt
            // screen (clear on exit), 1049 = save cursor + alt screen. The
            // grid implements a single alt-screen pair that saves the cursor
            // and swaps buffers, which covers all three — the discarded alt
            // buffer makes 47-vs-1047 clear-on-exit differences moot.
            47 | 1047 | 1049 => {
                if enable {
                    grid.enter_alt_screen();
                } else {
                    grid.exit_alt_screen();
                }
            }
            // DECSC/DECRC-equivalent cursor save/restore.
            1048 => {
                if enable {
                    grid.save_cursor();
                } else {
                    grid.restore_cursor();
                }
            }
            2004 => grid.bracketed_paste = enable,
            // ponytail: single log of unhandled modes; add SGR-mouse etc. only when a real TUI needs it
            mode => log::debug!("ignoring unsupported DEC private mode {mode} (enable={enable})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vte::Parser;

    fn parse(input: &[u8]) -> (Grid, Vec<Vec<u8>>) {
        let mut parser = Parser::new();
        let mut handler = AnsiHandler::new();
        parser.advance(&mut handler, input);
        let mut grid = Grid::new(20, 4);
        let responses = apply_ops_with_responses(&mut grid, handler.ops());
        (grid, responses)
    }

    #[test]
    fn dsr_reports_one_based_cursor_position() {
        let (_grid, responses) = parse(b"abc\x1b[6n");
        assert_eq!(responses, vec![b"\x1b[1;4R".to_vec()]);
    }

    #[test]
    fn private_dsr_is_not_answered_as_a_standard_cursor_query() {
        let (_grid, responses) = parse(b"\x1b[?6n");
        assert!(responses.is_empty());
    }

    #[test]
    fn hts_adds_a_tab_stop() {
        let (mut grid, responses) = parse(b"\x1b[2G\x1bH");
        assert!(responses.is_empty());
        grid.move_cursor_to_col(0);
        grid.tab();
        assert_eq!(grid.cursor.col, 1);
    }

    #[test]
    fn bs_is_non_destructive() {
        let (grid, _) = parse(b"abc\x08X");
        assert_eq!(grid.get_cell(0, 0).map(|c| c.c), Some('a'));
        assert_eq!(grid.get_cell(0, 1).map(|c| c.c), Some('b'));
        assert_eq!(grid.get_cell(0, 2).map(|c| c.c), Some('X'));
    }

    #[test]
    fn decstb_sets_scroll_region_and_scrolls_it() {
        let (mut grid, _) = parse(b"\x1b[2;3r");
        assert_eq!(grid.scroll_region.top, 1);
        assert_eq!(grid.scroll_region.bottom, 2);
        // Cursor homed by DECSTB.
        grid.newline();
        assert_eq!(grid.cursor.row, 1);
        // LF at the bottom margin scrolls the region instead of advancing.
        grid.newline();
        grid.newline();
        assert_eq!(grid.cursor.row, 2);
    }

    #[test]
    fn private_modes_toggle_cursor_paste_and_alt_screen() {
        let (grid, _) = parse(b"\x1b[?25l\x1b[?2004h");
        assert!(!grid.cursor_visible);
        assert!(grid.bracketed_paste);

        let (mut grid, _) = parse(b"ab\x1b[?1049h");
        assert_eq!(grid.get_cell(0, 0).map(|c| c.c), Some('\0'));
        let (grid, _) = {
            let _ = apply_ops_with_responses(&mut grid, {
                let mut parser = vte::Parser::new();
                let mut handler = AnsiHandler::new();
                parser.advance(&mut handler, b"\x1b[?1049l");
                handler.ops()
            });
            (grid, ())
        };
        assert_eq!(grid.get_cell(0, 0).map(|c| c.c), Some('a'));
    }

    #[test]
    fn alt_screen_family_47_and_1047_switch_buffers() {
        for mode in [b"\x1b[?47h".as_slice(), b"\x1b[?1047h".as_slice()] {
            let (mut grid, _) = parse(b"ab");
            let mut parser = vte::Parser::new();
            let mut handler = AnsiHandler::new();
            parser.advance(&mut handler, mode);
            let _ = apply_ops_with_responses(&mut grid, handler.ops());
            assert_eq!(grid.get_cell(0, 0).map(|c| c.c), Some('\0'));
        }
    }

    #[test]
    fn mode_1048_saves_and_restores_cursor() {
        let (mut grid, _) = parse(b"ab\x1b[?1048h");
        assert_eq!(grid.cursor.col, 2);
        grid.move_cursor_to_col(0);
        let mut parser = vte::Parser::new();
        let mut handler = AnsiHandler::new();
        parser.advance(&mut handler, b"\x1b[?1048l");
        let _ = apply_ops_with_responses(&mut grid, handler.ops());
        assert_eq!(grid.cursor.col, 2);
    }
}
