use super::{load_trusted_roots, TRUSTED_ROOTS_KEY};
use crate::state::AppState;
use tauri::State;

/// Event name emitted (from `store_set`) when the workspace store changes.
/// Kept here so the relay's event allowlist and the store command agree.
pub(crate) const WORKSPACE_CHANGED_EVENT: &str = "workspace:changed";

/// Add a directory to the set of trusted workspace roots.
///
/// This is the authorization gesture that lets a terminal or AI agent operate
/// in a directory outside the app's own project root. The directory must
/// exist and be a directory — a renderer cannot bless an arbitrary string;
/// it can only opt in to a real folder it could browse to anyway. Idempotent:
/// adding an already-trusted root is a no-op.
///
/// Canonicalizes before storing so later comparisons against a canonicalized
/// request path are exact, and so a symlinked path is stored as its target.
#[tauri::command]
pub async fn workspace_add_trusted_root(
    state: State<'_, AppState>,
    dir: String,
) -> Result<(), String> {
    let dir_for_task = dir.clone();
    let (canonical, is_dir) = tokio::task::spawn_blocking(move || {
        let p = std::path::Path::new(&dir_for_task);
        let canonical = p.canonicalize();
        let is_dir = match &canonical {
            Ok(c) => std::fs::metadata(c).map(|m| m.is_dir()).unwrap_or(false),
            Err(_) => false,
        };
        (canonical, is_dir)
    })
    .await
    .map_err(|e| format!("path resolve task failed: {e}"))?;

    let canonical = canonical.map_err(|e| format!("'{}' is not accessible: {}", dir, e))?;
    if !is_dir {
        return Err(format!("'{}' is not a directory", dir));
    }
    let mut roots = load_trusted_roots(&state.store);
    if roots.iter().any(|r| r == &canonical) {
        return Ok(());
    }
    roots.push(canonical);
    let strs: Vec<String> = roots
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    state
        .store
        .set_sync(TRUSTED_ROOTS_KEY, &strs)
        .map_err(|e| format!("failed to persist trusted root: {}", e))?;
    Ok(())
}

/// Remove a directory from the set of trusted workspace roots.
///
/// Accepts either the stored canonical form or any path that canonicalizes to
/// it, so the frontend doesn't have to know the exact stored string.
/// Removing a root that isn't trusted is a no-op.
#[tauri::command]
pub async fn workspace_remove_trusted_root(
    state: State<'_, AppState>,
    dir: String,
) -> Result<(), String> {
    let dir_for_task = dir.clone();
    let canonical =
        tokio::task::spawn_blocking(move || std::path::Path::new(&dir_for_task).canonicalize())
            .await
            .map_err(|e| format!("canonicalize task failed: {e}"))?;
    let canonical = canonical.ok();

    let mut roots = load_trusted_roots(&state.store);
    let before = roots.len();
    if let Some(ref c) = canonical {
        roots.retain(|r| r != c);
    }
    if canonical.is_none() {
        let lit = std::path::PathBuf::from(&dir);
        roots.retain(|r| r != &lit);
    }
    if roots.len() == before {
        return Ok(());
    }
    let strs: Vec<String> = roots
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    state
        .store
        .set_sync(TRUSTED_ROOTS_KEY, &strs)
        .map_err(|e| format!("failed to persist trusted roots: {}", e))?;
    Ok(())
}

/// List the canonicalized trusted workspace roots.
#[tauri::command]
pub fn workspace_list_trusted_roots(state: State<'_, AppState>) -> Vec<String> {
    load_trusted_roots(&state.store)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}
