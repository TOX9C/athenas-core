//! Pure terminal color model shared by the terminal store and cell renderer.

use serde::{Deserialize, Serialize};

/// Terminal color (ANSI 256 + true color support).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum TerminalColor {
    #[default]
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
// Backend Color raw representation (parsed from JSON)
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_color_maps_all_variants() {
        assert_eq!(
            backend_named_color_to_terminal(&BackendNamedColor::Black),
            TerminalColor::Black
        );
        assert_eq!(
            backend_named_color_to_terminal(&BackendNamedColor::Red),
            TerminalColor::Red
        );
        assert_eq!(
            backend_named_color_to_terminal(&BackendNamedColor::BrightWhite),
            TerminalColor::BrightWhite
        );
        // Foreground/Background collapse to Default (no dedicated frontend variant).
        assert_eq!(
            backend_named_color_to_terminal(&BackendNamedColor::Foreground),
            TerminalColor::Default
        );
        assert_eq!(
            backend_named_color_to_terminal(&BackendNamedColor::Background),
            TerminalColor::Default
        );
    }

    #[test]
    fn raw_color_round_trips_each_shape() {
        // Bare "Default" string.
        assert_eq!(
            backend_color_raw_to_terminal(&BackendColorRaw::DefaultString("Default".to_string())),
            TerminalColor::Default
        );
        // Named variant.
        assert_eq!(
            backend_color_raw_to_terminal(&BackendColorRaw::Named(BackendColorNamed {
                Named: BackendNamedColor::Cyan,
            })),
            TerminalColor::Cyan
        );
        // Indexed variant.
        assert_eq!(
            backend_color_raw_to_terminal(&BackendColorRaw::Indexed(BackendColorIndexed {
                Indexed: 128,
            })),
            TerminalColor::Indexed(128)
        );
        // Rgb variant.
        assert_eq!(
            backend_color_raw_to_terminal(&BackendColorRaw::Rgb(BackendColorRgb {
                Rgb: [255, 128, 0],
            })),
            TerminalColor::Rgb(255, 128, 0)
        );
    }
}
