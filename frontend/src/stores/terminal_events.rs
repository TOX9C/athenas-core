//! Backend terminal event contracts and cell flags.

use serde::{Deserialize, Serialize};

use super::TerminalCell;

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
    pub fg: super::BackendColorRaw,
    pub bg: super::BackendColorRaw,
    pub flags: u16,
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

/// Delta update from the backend (mirrors athena-terminal TerminalUpdate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TerminalUpdateDelta {
    pub start_y: usize,
    pub rows: Vec<Vec<TerminalCell>>,
    pub cursor_pos: Option<(usize, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_bits_are_distinct() {
        let flags = [
            FLAGS_INVERSE,
            FLAGS_BOLD,
            FLAGS_ITALIC,
            FLAGS_UNDERLINE,
            FLAGS_STRIKEOUT,
            FLAGS_BLINK,
        ];
        for (index, flag) in flags.iter().enumerate() {
            assert!(flags
                .iter()
                .enumerate()
                .all(|(other, other_flag)| index == other || flag & other_flag == 0));
        }
    }

    #[test]
    fn event_contracts_default_to_empty() {
        let event = TerminalDataEvent {
            sessionId: String::new(),
            deltas: Vec::new(),
            cursorRow: 0,
            cursorCol: 0,
            rows: 0,
            cols: 0,
            cursorVisible: None,
        };
        assert!(event.deltas.is_empty());
        assert_eq!(TerminalUpdateDelta::default().start_y, 0);
        assert!(TerminalUpdateDelta::default().rows.is_empty());
    }
}
