use super::{load_trusted_roots, TRUSTED_ROOTS_KEY};
use crate::state::AppState;
use tauri::State;

/// Event name emitted (from `store_set`) when the workspace store changes.
/// Kept here so the relay's event allowlist and the store command agree.
pub(crate) const WORKSPACE_CHANGED_EVENT: &str = "workspace:changed";

/// Reject system-sensitive roots that would neutralize the path sandbox.
///
/// A trusted root grants every sandboxed command (fs_*, pty_*, …) access to
/// everything beneath it, so blessing `/`, the entire home directory, or a
/// system directory is equivalent to disabling the sandbox — and it persists
/// across restarts. These can only ever be requested by a compromised or
/// buggy renderer, never by a user who needs a real project folder.
fn is_forbidden_root(canonical: &std::path::Path) -> bool {
    /// Directory prefixes that must never be trusted roots. `/` is handled
    /// separately (it has no parent, and `starts_with("/")` matches all).
    const FORBIDDEN_PREFIXES: &[&str] = &[
        "/bin", "/sbin", "/usr", "/etc", "/private", "/System", "/var", "/dev", "/tmp",
    ];
    if canonical.parent().is_none() {
        return true; // filesystem root ("/")
    }
    if let Some(home) = std::env::var_os("HOME") {
        if canonical == std::path::Path::new(&home) {
            return true; // the entire home directory
        }
    }
    FORBIDDEN_PREFIXES
        .iter()
        .any(|p| canonical.starts_with(std::path::Path::new(p)))
}

/// Add a directory to the set of trusted workspace roots.
///
/// This is the authorization gesture that lets a terminal or AI agent operate
/// in a directory outside the app's own project root. Because one IPC call
/// with `dir="/"` would permanently disable the whole path sandbox, the
/// gesture is two-fold and both halves are enforced here, in the backend:
///
/// 1. The directory must not be a system-sensitive root (`/`, `$HOME`,
///    `/bin`, `/usr`, `/etc`, `/private`, …) — these are rejected outright.
/// 2. The user must confirm a native desktop dialog naming the exact
///    directory. The renderer is expected to drive this through the native
///    folder picker (`fs_show_open_dialog` with `directory: true`) and pass
///    the picked path here; a programmatic caller cannot bypass the backend
///    confirmation dialog.
///
/// The directory must exist and be a directory. Idempotent: adding an
/// already-trusted root is a no-op (and skips the dialog).
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
    if is_forbidden_root(&canonical) {
        log::warn!(
            "workspace_add_trusted_root rejected system root: {}",
            canonical.display()
        );
        return Err(format!(
            "'{}' is a system or home root and cannot be trusted; pick a project folder inside it",
            canonical.display()
        ));
    }
    let mut roots = load_trusted_roots(&state.store);
    if roots.iter().any(|r| r == &canonical) {
        return Ok(());
    }

    // Backend-enforced user gesture: the exact canonical directory the
    // renderer (or relay) asked to bless is shown in a native dialog, and the
    // root is only added when the user confirms. This runs on a blocking
    // thread because the dialog API is synchronous.
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    let app = state
        .get_app_handle()
        .ok_or_else(|| "app not ready; cannot confirm trusted root".to_string())?;
    let path_display = canonical.display().to_string();
    let confirmed = tokio::task::spawn_blocking(move || {
        app.dialog()
            .message(format!(
                "Trust this folder as a workspace root?\n\n{path_display}\n\nAthena terminals and agents will be allowed to read and write inside it."
            ))
            .title("Trust Folder")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Trust Folder".to_string(),
                "Cancel".to_string(),
            ))
            .blocking_show()
    })
    .await
    .map_err(|e| format!("confirmation dialog failed: {e}"))?;
    if !confirmed {
        return Err("user declined to trust this folder".to_string());
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
