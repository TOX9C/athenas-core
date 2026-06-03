use super::CommandError;

/// Search the codebase for a pattern using ripgrep.
#[tauri::command]
pub async fn search_code(pattern: String, path: String) -> Result<String, CommandError> {
    if pattern.is_empty() {
        return Err(CommandError::InvalidInput("Search pattern cannot be empty".into()));
    }
    if pattern.len() > 4096 {
        return Err(CommandError::InvalidInput("Search pattern too long".into()));
    }
    if path.is_empty() {
        return Err(CommandError::InvalidInput("Search path cannot be empty".into()));
    }
    let options = athena_core::SearchOptions {
        pattern,
        path,
        glob: None,
        case_sensitive: false,
        max_results: Some(50),
        context_lines: Some(2),
    };
    let result = athena_core::search_code(&options).await.map_err(|e| CommandError::Internal(e.to_string()))?;
    serde_json::to_string(&result).map_err(|e| CommandError::Internal(e.to_string()))
}

/// Search the codebase using ripgrep (alias for search_code).
#[tauri::command]
pub async fn search_ripgrep(pattern: String, path: String) -> Result<String, CommandError> {
    search_code(pattern, path).await
}
