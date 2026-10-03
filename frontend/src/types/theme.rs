/// Mythology-themed palettes plus popular community palettes.
/// Dark: Nyx, Aegis, Erebus, Dracula, Nord, Catppuccin Mocha, Gruvbox.
/// Light: Pentelic, Olive, Sky, Catppuccin Latte, Solarized.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UITheme {
    #[default]
    Nyx,
    Aegis,
    Erebus,
    Pentelic,
    Olive,
    Sky,
    Dracula,
    Nord,
    CatppuccinMocha,
    Gruvbox,
    CatppuccinLatte,
    Solarized,
    System,
}

impl UITheme {
    pub fn is_dark(&self) -> bool {
        matches!(
            self,
            UITheme::Nyx
                | UITheme::Aegis
                | UITheme::Erebus
                | UITheme::Dracula
                | UITheme::Nord
                | UITheme::CatppuccinMocha
                | UITheme::Gruvbox
        )
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
            UITheme::Dracula => "dracula",
            UITheme::Nord => "nord",
            UITheme::CatppuccinMocha => "catppuccin-mocha",
            UITheme::Gruvbox => "gruvbox",
            UITheme::CatppuccinLatte => "catppuccin-latte",
            UITheme::Solarized => "solarized",
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
            UITheme::Dracula => "Dracula",
            UITheme::Nord => "Nord",
            UITheme::CatppuccinMocha => "Catppuccin Mocha",
            UITheme::Gruvbox => "Gruvbox",
            UITheme::CatppuccinLatte => "Catppuccin Latte",
            UITheme::Solarized => "Solarized Light",
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
            "dracula" => UITheme::Dracula,
            "nord" => UITheme::Nord,
            "catppuccin-mocha" => UITheme::CatppuccinMocha,
            "gruvbox" => UITheme::Gruvbox,
            "catppuccin-latte" => UITheme::CatppuccinLatte,
            "solarized" => UITheme::Solarized,
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
            UITheme::Dracula,
            UITheme::Nord,
            UITheme::CatppuccinMocha,
            UITheme::Gruvbox,
            UITheme::CatppuccinLatte,
            UITheme::Solarized,
            UITheme::System,
        ]
    }
}
