//! File icon utility — ported from src/utils/fileIcons.ts
//!
//! Maps file extensions to emoji icons.

use once_cell::sync::Lazy;
use std::collections::HashMap;

static ICON_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("ts", "🔷");
    m.insert("tsx", "⚛️");
    m.insert("js", "🟡");
    m.insert("jsx", "⚛️");
    m.insert("json", "📋");
    m.insert("md", "📝");
    m.insert("css", "🎨");
    m.insert("scss", "🎨");
    m.insert("html", "🌐");
    m.insert("svg", "🖼️");
    m.insert("png", "🖼️");
    m.insert("jpg", "🖼️");
    m.insert("jpeg", "🖼️");
    m.insert("gif", "🖼️");
    m.insert("webp", "🖼️");
    m.insert("yml", "⚙️");
    m.insert("yaml", "⚙️");
    m.insert("toml", "⚙️");
    m.insert("sh", "🐚");
    m.insert("bash", "🐚");
    m.insert("zsh", "🐚");
    m.insert("py", "🐍");
    m.insert("go", "🔵");
    m.insert("rs", "🦀");
    m.insert("rb", "💎");
    m.insert("lock", "🔒");
    m.insert("gitignore", "👁️");
    m
});

/// Get the emoji icon for a file based on its extension.
pub fn get_file_icon(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    ICON_MAP.get(ext.as_str()).copied().unwrap_or("📄")
}
