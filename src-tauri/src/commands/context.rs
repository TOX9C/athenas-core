//! Project-convention context (AGENTS.md/CLAUDE.md/.cursorrules/.goosehints)
//! discovery + scoped application: chat reads a merged block, swarms get the
//! files written into each agent worktree.

use super::{validate_path_exists, CommandError};
use crate::state::AppState;
use tauri::State;

/// The KV key stores per-file opt-in flags: `{ "path": {"chat": bool, "swarm": bool} }`.
pub(crate) const CONTEXT_SELECTION_KEY: &str = "project_context.selection";

fn path_enabled(state: &AppState, path: &str, chat: bool) -> bool {
    let sel: Option<serde_json::Value> = state.store.get(CONTEXT_SELECTION_KEY).ok().flatten();
    sel.as_ref()
        .and_then(|v| v.get(path))
        .and_then(|v| v.get(if chat { "chat" } else { "swarm" }))
        .and_then(|v| v.as_bool())
        // Default: context participates in both chat and swarm.
        .unwrap_or(true)
}

/// Files found in the workspace (parents up to home), each with bytes and
/// per-scope enablement.
#[tauri::command]
pub async fn project_context_list(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<serde_json::Value>, CommandError> {
    if !state.rate_limiter.check("project_context_list") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let path_ref = std::path::Path::new(&path);
    let validated = validate_path_exists(&state.store, path_ref)?;
    // Selection snapshot BEFORE the spawn_blocking (`State<'_>` doesn't live
    // long enough to cross the await boundary).
    let sel: Option<serde_json::Value> =
        state.store.get::<serde_json::Value>(CONTEXT_SELECTION_KEY).ok().flatten();
    tokio::task::spawn_blocking(move || {
        let files = athena_core::project_context::discover(&validated);
        let is_enabled = |path: &str, chat: bool| -> bool {
            sel.as_ref()
                .and_then(|v| v.get(path))
                .and_then(|v| v.get(if chat { "chat" } else { "swarm" }))
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        };
        let out: Vec<serde_json::Value> = files
            .into_iter()
            .map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "name": f.name,
                    "bytes": f.bytes,
                    "chat": is_enabled(&f.path, true),
                    "swarm": is_enabled(&f.path, false),
                })
            })
            .collect();
        Ok(out)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("context list task failed: {e}")))?
}

/// Merge the chat-enabled context files into the orchestrator's session
/// context. No-op when no files are discovered/selected.
pub(crate) fn apply_chat_context(state: &AppState, workspace_dir: &str) {
    let Ok(root) = validate_path_exists(&state.store, std::path::Path::new(workspace_dir)) else {
        return;
    };
    let files = athena_core::project_context::discover(&root);
    let paths: Vec<_> = files
        .iter()
        .filter(|f| path_enabled(state, &f.path, true))
        .map(|f| std::path::PathBuf::from(&f.path))
        .collect();
    if paths.is_empty() {
        state.orchestrator.set_project_context(None);
        return;
    }
    let refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
    let merged = athena_core::project_context::read_concat(&refs);
    state
        .orchestrator
        .set_project_context(if merged.trim().is_empty() { None } else { Some(merged) });
}

/// Write every swarm-enabled context file into each worktree (existing files
/// in the worktree — e.g. a committed CLAUDE.md — are never overwritten).
#[tauri::command]
pub async fn project_context_apply(
    state: State<'_, AppState>,
    path: String,
) -> Result<usize, CommandError> {
    if !state.rate_limiter.check("project_context_apply") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let validated = validate_path_exists(&state.store, std::path::Path::new(&path))?;
    let files = athena_core::project_context::discover(&validated);
    let paths: Vec<_> = files
        .iter()
        .filter(|f| path_enabled(&state, &f.path, false))
        .map(|f| std::path::PathBuf::from(&f.path))
        .collect();
    tokio::task::spawn_blocking(move || {
        if paths.is_empty() {
            return Ok(0);
        }
        // Convention files are copied next to every agent worktree under the
        // workspace's .athena/worktrees/ only — never outside the sandbox.
        let wt_root = validated.join(".athena/worktrees");
        if !wt_root.exists() {
            return Ok(0);
        }
        let mut written = 0usize;
        for entry in std::fs::read_dir(&wt_root)
            .map_err(|e| CommandError::Internal(format!("cannot read worktrees dir: {e}")))?
        {
            let entry = entry.map_err(|e| CommandError::Internal(e.to_string()))?;
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
            written += athena_core::project_context::copy_into_worktree(&dir, &refs)
                .map_err(CommandError::Internal)?;
        }
        Ok(written)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("context apply task failed: {e}")))?
}
