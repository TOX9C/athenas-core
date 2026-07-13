/// Mythology-themed palettes. Dark: Nyx (obsidian+gold), Aegis (Aegean blue+bronze),
/// Erebus (true black+gold-leaf). Light: Pentelic (marble), Olive (parchment), Sky (cool marble).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UITheme {
    #[default]
    Nyx,
    Aegis,
    Erebus,
    Pentelic,
    Olive,
    Sky,
    System,
}

impl UITheme {
    pub fn is_dark(&self) -> bool {
        matches!(self, UITheme::Nyx | UITheme::Aegis | UITheme::Erebus)
    }

    /// Lowercase id — used as the persisted store key and palette lookup key.
    pub fn name(&self) -> &'static str {
        match self {
            UITheme::Nyx => "nyx",
            UITheme::Aegis => "aegis",
            UITheme::Erebus => "erebus",
            UITheme::Pentelic => "pentelic",
            UITheme::Olive => "olive",
            UITheme::Sky => "sky",
            UITheme::System => "system",
        }
    }

    /// Display label (capitalized) for UI surfaces.
    pub fn label(&self) -> &'static str {
        match self {
            UITheme::Nyx => "Nyx",
            UITheme::Aegis => "Aegis",
            UITheme::Erebus => "Erebus",
            UITheme::Pentelic => "Pentelic",
            UITheme::Olive => "Olive",
            UITheme::Sky => "Sky",
            UITheme::System => "System",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "nyx" => UITheme::Nyx,
            "aegis" => UITheme::Aegis,
            "erebus" => UITheme::Erebus,
            "pentelic" => UITheme::Pentelic,
            "olive" => UITheme::Olive,
            "sky" => UITheme::Sky,
            "system" => UITheme::System,
            _ => UITheme::Nyx,
        }
    }

    pub fn all() -> &'static [UITheme] {
        &[
            UITheme::Nyx,
            UITheme::Aegis,
            UITheme::Erebus,
            UITheme::Pentelic,
            UITheme::Olive,
            UITheme::Sky,
            UITheme::System,
        ]
    }
}
