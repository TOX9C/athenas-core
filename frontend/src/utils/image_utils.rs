//! Image utility functions — ported from src/utils/imageUtils.ts
//!
//! The TS version uses browser APIs (File, Image, Canvas, FileReader).
//! This Rust version provides equivalent logic for validation and base64
//! extraction, while image compression/resize would use the `image` crate
//! on the Tauri backend side.

use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Default maximum image width for compression.
pub const DEFAULT_MAX_WIDTH: u32 = 2048;

/// Default maximum image height for compression.
pub const DEFAULT_MAX_HEIGHT: u32 = 2048;

/// Default JPEG quality for compression (0.0–1.0).
pub const DEFAULT_QUALITY: f32 = 0.8;

/// Default maximum file size in megabytes.
pub const DEFAULT_MAX_SIZE_MB: u32 = 10;

static VALID_IMAGE_TYPES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "image/png",
        "image/jpeg",
        "image/jpg",
        "image/gif",
        "image/webp",
        "image/bmp",
    ]
    .iter()
    .cloned()
    .collect()
});

static VALID_IMAGE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    ["png", "jpeg", "jpg", "gif", "webp", "bmp"]
        .iter()
        .cloned()
        .collect()
});

/// Result of image validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub error: Option<String>,
}

/// Check if a MIME type or file extension indicates an image file.
pub fn is_image_file(mime_type: Option<&str>, filename: Option<&str>) -> bool {
    if let Some(mt) = mime_type {
        if VALID_IMAGE_TYPES.contains(mt) {
            return true;
        }
    }
    if let Some(name) = filename {
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        return VALID_IMAGE_EXTENSIONS.contains(ext.as_str());
    }
    false
}

/// Validate an image by MIME type, extension, and size.
pub fn validate_image(
    mime_type: Option<&str>,
    filename: Option<&str>,
    size_bytes: u64,
    max_size_mb: Option<u32>,
) -> ValidationResult {
    let max_mb = max_size_mb.unwrap_or(DEFAULT_MAX_SIZE_MB);
    if !is_image_file(mime_type, filename) {
        return ValidationResult {
            valid: false,
            error: Some("File is not a supported image type.".to_string()),
        };
    }
    if size_bytes > (max_mb as u64) * 1024 * 1024 {
        return ValidationResult {
            valid: false,
            error: Some(format!("Image exceeds maximum size of {}MB.", max_mb)),
        };
    }
    ValidationResult {
        valid: true,
        error: None,
    }
}

/// Parsed base64 data from a data URI.
#[derive(Debug, Clone)]
pub struct Base64Data {
    pub media_type: String,
    pub base64: String,
}

/// Extract the media type and base64 payload from a data URI
/// (`data:<mediatype>;base64,<data>`).
pub fn extract_base64_data(data_uri: &str) -> Result<Base64Data, String> {
    if !data_uri.starts_with("data:") {
        return Err("Invalid data URI: must start with 'data:'".to_string());
    }
    let rest = &data_uri[5..];
    let (header, data) = rest
        .split_once(',')
        .ok_or("Invalid data URI: missing comma separator")?;
    let media_type = header
        .strip_suffix(";base64")
        .ok_or("Invalid data URI: missing ';base64' suffix")?
        .to_string();
    Ok(Base64Data {
        media_type,
        base64: data.to_string(),
    })
}
