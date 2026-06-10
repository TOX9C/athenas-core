//! File icon utility — ported from src/utils/fileIcons.ts
//!
//! Maps file extensions to emoji icons.

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
