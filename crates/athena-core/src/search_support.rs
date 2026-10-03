//! Shared validation, limits, errors, and ripgrep discovery.

use std::path::PathBuf;

/// Hard limits for search parameters to prevent DoS.
pub(super) const MAX_CONTEXT_LINES: usize = 100;
pub(super) const MAX_RESULTS: usize = 5000;

/// Locate the ripgrep binary, falling back to common system paths.
pub(crate) async fn find_rg_binary() -> Result<PathBuf, SearchError> {
    // Absolute install prefixes only. A bare `"rg"`/`"rg.exe"` candidate
    // would run `PathBuf::from("rg").exists()`, which tests the CWD, not
    // PATH — the actual PATH lookup is the `which`/`where` probe below.
    let candidates: Vec<&str> = if cfg!(windows) {
        vec![]
    } else {
        vec!["/usr/local/bin/rg", "/opt/homebrew/bin/rg", "/usr/bin/rg"]
    };

    for candidate in &candidates {
        let candidate_path = PathBuf::from(candidate);
        if candidate_path.exists() {
            return Ok(candidate_path);
        }
    }

    // Try to find via `which` as last resort
    let which_result = if cfg!(windows) {
        tokio::process::Command::new("cmd")
            .args(["/c", "where", "rg"])
            .output()
            .await
    } else {
        tokio::process::Command::new("which")
            .arg("rg")
            .output()
            .await
    };

    if let Ok(output) = which_result {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err(SearchError::RgNotFound)
}

/// Errors that can occur during search operations.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error(
        "ripgrep binary not found. Install it via your package manager (e.g. brew install ripgrep)"
    )]
    RgNotFound,
    #[error("Failed to spawn ripgrep: {0}")]
    SpawnError(#[from] std::io::Error),
    #[error("ripgrep process exited with code {code}: {stderr}")]
    RgExit { code: i32, stderr: String },
    #[error("JSON parse error: {0}")]
    JsonParseError(#[from] serde_json::Error),
    #[error("Invalid search pattern: {0}")]
    InvalidPattern(String),
}

/// Validate that a regex/search pattern is safe to pass to ripgrep and won't
/// cause catastrophic backtracking (CPU-based DoS).
pub(super) fn validate_pattern(pattern: &str) -> Result<(), SearchError> {
    if pattern.len() > 1024 {
        return Err(SearchError::InvalidPattern(
            "pattern too long (max 1024 chars)".to_string(),
        ));
    }
    let repeat_ops = pattern.matches('{').count() + pattern.matches('*').count();
    if repeat_ops > 10 {
        return Err(SearchError::InvalidPattern(
            "pattern contains too many repetition operators".to_string(),
        ));
    }
    Ok(())
}
