use super::validate_path_exists;
use crate::state::AppState;
use tauri::State;

/// Search the codebase for a pattern using ripgrep.
#[tauri::command]
pub async fn search_code(
    state: State<'_, AppState>,
    pattern: String,
    path: String,
) -> Result<String, String> {
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref).map_err(|e| e.to_string())?;
    let options = athena_core::SearchOptions {
        pattern,
        path: validated.to_string_lossy().to_string(),
        glob: None,
        case_sensitive: false,
        max_results: Some(50),
        context_lines: Some(2),
    };
    let result = athena_core::search_code(&options)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Search the codebase using ripgrep (alias for search_code).
#[tauri::command]
pub async fn search_ripgrep(
    state: State<'_, AppState>,
    pattern: String,
    path: String,
) -> Result<String, String> {
    search_code(state, pattern, path).await
}
