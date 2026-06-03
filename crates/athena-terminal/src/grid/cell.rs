use crate::grid::colors::Color;
use bitflags::bitflags;

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CellFlags: u16 {
        const INVERSE      = 0b0000_0000_0000_0001;
        const BOLD         = 0b0000_0000_0000_0010;
        const ITALIC       = 0b0000_0000_0000_0100;
        const UNDERLINE    = 0b0000_0000_0000_1000;
        const WRAPLINE     = 0b0000_0000_0001_0000;
        const WIDE_CHAR    = 0b0000_0000_0010_0000;
        const DIM          = 0b0000_0000_0100_0000;
        const STRIKEOUT    = 0b0000_0000_1000_0000;
        const BLINK        = 0b0000_0001_0000_0000;
        const INVISIBLE    = 0b0000_0010_0000_0000;
        const DOUBLE_UNDERLINE = 0b0000_0100_0000_0000;
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CellExtraFlags: u8 {
        const HYPERLINK    = 0b0000_0001;
        const BOLD_FAKE    = 0b0000_0010;
        const ITALIC_FAKE  = 0b0000_0100;
    }
}

/// A single terminal cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
    pub extra: Option<Box<CellExtra>>,
}

impl Cell {
    pub fn new(c: char) -> Self {
        Self {
            c,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.c == '\0' || self.c == ' '
    }

    pub fn reset(&mut self, template: &Cell) {
        self.c = '\0';
        self.fg = template.fg.clone();
        self.bg = template.bg.clone();
        self.flags = template.flags;
        self.extra = None;
    }

    pub fn clone_with_char(&self, c: char) -> Self {
        let mut cell = self.clone();
        cell.c = c;
        cell
    }
}

/// Extra attributes for a cell (allocated lazily).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellExtra {
    pub zero_width_chars: Vec<char>,
    pub hyperlink_id: Option<u16>,
    pub flags: CellExtraFlags,
}
