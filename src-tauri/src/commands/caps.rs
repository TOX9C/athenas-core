//! Size and content caps for Tauri command inputs.
//!
//! Centralized here so all command handlers share a single source of truth
//! for the maximum sizes we'll accept from the frontend. Without these caps
//! an attacker (or runaway client) can exhaust memory, fill disk, or pollute
//! the SQLite-backed key store via unbounded string payloads.

/// Maximum bytes accepted by `fs_write_file` (10 MB).
pub const MAX_FS_WRITE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum bytes accepted by `mcp_handle_request` (1 MB).
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

/// Maximum bytes accepted by `shell_integration_parse` (1 MB).
pub const MAX_DATA_BYTES: usize = 1024 * 1024;

/// Maximum character length for a chat session title.
pub const MAX_SESSION_TITLE_LEN: usize = 256;

/// Maximum character length for a key-value store key.
pub const MAX_STORE_KEY_LEN: usize = 1024;

/// Validate a store key: length cap plus rejection of control characters
/// (except tab, which is permitted for hierarchical key naming).
pub fn validate_key(key: &str) -> Result<(), String> {
    if key.len() > MAX_STORE_KEY_LEN {
        return Err(format!(
            "key too long: {} > {}",
            key.len(),
            MAX_STORE_KEY_LEN
        ));
    }
    if key.chars().any(|c| c.is_control() && c != '\t') {
        return Err("key contains control characters".to_string());
    }
    Ok(())
}

/// Validate a session title: length cap only.
pub fn validate_title(title: &str) -> Result<(), String> {
    if title.len() > MAX_SESSION_TITLE_LEN {
        return Err(format!(
            "title too long: {} > {}",
            title.len(),
            MAX_SESSION_TITLE_LEN
        ));
    }
    Ok(())
}
