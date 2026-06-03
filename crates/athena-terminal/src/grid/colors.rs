use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    /// The default foreground/background color.
    #[default]
    Default,
    /// A named color from the standard 16-color palette.
    Named(NamedColor),
    /// An indexed color from the 256-color palette.
    Indexed(u8),
    /// A true-color RGB value.
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamedColor {
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    #[default]
    White = 7,
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
    Foreground = 256,
    Background = 257,
}

impl NamedColor {
    pub fn from_ansi(idx: u8) -> Self {
        match idx {
            0 => Self::Black,
            1 => Self::Red,
            2 => Self::Green,
            3 => Self::Yellow,
            4 => Self::Blue,
            5 => Self::Magenta,
            6 => Self::Cyan,
            7 => Self::White,
            8 => Self::BrightBlack,
            9 => Self::BrightRed,
            10 => Self::BrightGreen,
            11 => Self::BrightYellow,
            12 => Self::BrightBlue,
            13 => Self::BrightMagenta,
            14 => Self::BrightCyan,
            15 => Self::BrightWhite,
            _ => Self::White,
        }
    }

    pub fn as_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Black => (0, 0, 0),
            Self::Red => (205, 49, 49),
            Self::Green => (13, 188, 121),
            Self::Yellow => (229, 229, 16),
            Self::Blue => (36, 114, 200),
            Self::Magenta => (188, 63, 188),
            Self::Cyan => (17, 168, 205),
            Self::White => (229, 229, 229),
            Self::BrightBlack => (102, 102, 102),
            Self::BrightRed => (241, 76, 76),
            Self::BrightGreen => (35, 209, 139),
            Self::BrightYellow => (245, 245, 67),
            Self::BrightBlue => (59, 142, 234),
            Self::BrightMagenta => (214, 112, 214),
            Self::BrightCyan => (41, 184, 219),
            Self::BrightWhite => (255, 255, 255),
            Self::Foreground => (229, 229, 229),
            Self::Background => (0, 0, 0),
        }
    }
}
