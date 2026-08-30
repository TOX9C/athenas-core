//! File icon utility — ported from src/utils/fileIcons.ts
//!
//! Maps file extensions to short text badges used in the file tree.

use once_cell::sync::Lazy;
use std::collections::HashMap;

static ICON_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("ts", "TS");
    m.insert("tsx", "TSX");
    m.insert("js", "JS");
    m.insert("jsx", "JSX");
    m.insert("json", "JSON");
    m.insert("md", "MD");
    m.insert("css", "CSS");
    m.insert("scss", "SCSS");
    m.insert("html", "HTML");
    m.insert("svg", "SVG");
    m.insert("png", "PNG");
    m.insert("jpg", "JPG");
    m.insert("jpeg", "JPG");
    m.insert("gif", "GIF");
    m.insert("webp", "WEBP");
    m.insert("yml", "YML");
    m.insert("yaml", "YML");
    m.insert("toml", "TOML");
    m.insert("sh", "SH");
    m.insert("bash", "SH");
    m.insert("zsh", "SH");
    m.insert("py", "PY");
    m.insert("go", "GO");
    m.insert("rs", "RS");
    m.insert("rb", "RB");
    m.insert("lock", "LCK");
    m.insert("gitignore", "GIT");
    m
});

/// Get a short text abbreviation for a file based on its extension.
pub fn get_file_icon(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    ICON_MAP.get(ext.as_str()).copied().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract: known extensions map to their badge, case-insensitively;
    // unknown or extension-less names yield an empty badge, never panic.
    #[test]
    fn known_extensions_map_to_expected_badges() {
        assert_eq!(get_file_icon("main.ts"), "TS");
        assert_eq!(get_file_icon("App.tsx"), "TSX");
        assert_eq!(get_file_icon("lib.rs"), "RS");
        assert_eq!(get_file_icon("Cargo.toml"), "TOML");
        assert_eq!(get_file_icon("photo.JPEG"), "JPG");
        assert_eq!(get_file_icon(".gitignore"), "GIT");
    }

    #[test]
    fn matching_is_case_insensitive_on_extension() {
        assert_eq!(get_file_icon("README.MD"), "MD");
        assert_eq!(get_file_icon("notes.Md"), "MD");
    }

    #[test]
    fn unknown_or_missing_extension_yields_empty_badge() {
        assert_eq!(get_file_icon("archive.tar.gz"), ""); // only last ext considered
        assert_eq!(get_file_icon("Makefile"), ""); // no dot at all
        assert_eq!(get_file_icon("weird.zzzz"), "");
    }

    #[test]
    fn dotfile_last_segment_is_treated_as_extension() {
        // "filename.rsplit('.')" on ".gitignore" yields "gitignore" -> GIT.
        assert_eq!(get_file_icon(".gitignore"), "GIT");
    }
}
