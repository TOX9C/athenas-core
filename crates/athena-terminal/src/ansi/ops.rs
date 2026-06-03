/// Represents an ANSI operation that was parsed from the PTY output.
/// These are collected during VTE parsing and applied in batch.
#[derive(Debug, Clone)]
pub enum AnsiOp {
    /// Insert a printable character
    Print(char),
    /// Execute a C0/C1 control code
    Execute(u8),
    /// CSI sequence with parameters
    Csi {
        params: Vec<u16>,
        intermediates: Vec<u8>,
        ignore: bool,
        action: char,
    },
    /// OSC sequence
    Osc {
        params: Vec<Vec<u8>>,
        bell_terminated: bool,
    },
    /// ESC sequence
    Esc {
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    /// DCS sequence start (hook)
    DcsHook {
        params: Vec<u16>,
        intermediates: Vec<u8>,
        ignore: bool,
        action: char,
    },
    /// DCS sequence data
    DcsPut(u8),
    /// DCS sequence end (unhook)
    DcsUnhook,
}
