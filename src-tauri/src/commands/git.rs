use super::{validate_path_exists, CommandError};
use crate::state::AppState;
use tauri::State;

/// Validate `path`, then validate the discovered repository root as well:
/// a worktree found by walking *above* the trusted root would otherwise let
/// git commands read repo state outside the path sandbox.
fn validated_repo_root(
    state: &AppState,
    path: &str,
) -> Result<std::path::PathBuf, CommandError> {
    let validated = validate_path_exists(&state.store, std::path::Path::new(path))?;
    let root = athena_git::discover_repo(&validated)
        .map_err(map_git_err)?
        .ok_or_else(|| CommandError::InvalidInput(format!("not a git repository: {path}")))?;
    validate_path_exists(&state.store, &root)
}

fn map_git_err(e: athena_git::GitError) -> CommandError {
    match e {
        athena_git::GitError::NotARepo(p) => {
            CommandError::InvalidInput(format!("not a git repository: {p}"))
        }
        athena_git::GitError::Git(msg) => CommandError::Internal(msg),
    }
}

/// Return the repository root containing `path`, or `None` when the path is
/// not inside a git repository (or the repo root lies outside the sandbox).
#[tauri::command]
pub async fn git_discover(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<String>, CommandError> {
    if !state.rate_limiter.check("git_discover") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    match validated_repo_root(&state, &path) {
        Ok(root) => Ok(Some(root.display().to_string())),
        Err(CommandError::InvalidInput(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Branch + per-file staged/unstaged status for the repo containing `path`.
#[tauri::command]
pub async fn git_status(
    state: State<'_, AppState>,
    path: String,
) -> Result<athena_git::RepoStatus, CommandError> {
    if !state.rate_limiter.check("git_status") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || athena_git::status(&root).map_err(map_git_err))
        .await
        .map_err(|e| CommandError::Internal(format!("git status task failed: {e}")))?
}

/// Create `name`'s agent worktree (`<root>/.athena/worktrees/<name>` on
/// branch `athena/<name>`). Returns the absolute worktree path.
#[tauri::command]
pub async fn git_worktree_add(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> Result<String, CommandError> {
    if !state.rate_limiter.check("git_worktree_add") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || {
        athena_git::add_worktree(&root, &name)
            .map(|p| p.display().to_string())
            .map_err(map_git_err)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("git worktree add task failed: {e}")))?
}

/// Remove the worktree named `name` (created via `git_worktree_add`). The
/// backing branch is kept so agent commits are never destroyed.
#[tauri::command]
pub async fn git_worktree_remove(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> Result<(), CommandError> {
    if !state.rate_limiter.check("git_worktree_remove") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || {
        athena_git::remove_worktree(&root, &name).map_err(map_git_err)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("git worktree remove task failed: {e}")))?
}

/// Unified diff (`staged = true`: index vs HEAD, else workdir vs index) for
/// the repo containing `path`, capped at [`athena_git::MAX_DIFF_BYTES`].
#[tauri::command]
pub async fn git_diff(
    state: State<'_, AppState>,
    path: String,
    staged: bool,
) -> Result<athena_git::DiffSet, CommandError> {
    if !state.rate_limiter.check("git_diff") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || athena_git::diff(&root, staged).map_err(map_git_err))
        .await
        .map_err(|e| CommandError::Internal(format!("git diff task failed: {e}")))?
}

/// Unified diff of a single file (`staged` index vs HEAD, else workdir vs
/// index), capped like `git_diff`.
#[tauri::command]
pub async fn git_diff_file(
    state: State<'_, AppState>,
    path: String,
    staged: bool,
    file: String,
) -> Result<Option<athena_git::FileDiff>, CommandError> {
    if !state.rate_limiter.check("git_diff_file") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || {
        athena_git::diff_file(&root, staged, &file).map_err(map_git_err)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("git diff file task failed: {e}")))?
}

/// Apply a single hunk of a file's unstaged diff: `action` is "stage"
/// (partial stage into the index) or "discard" (partial revert in the
/// working tree).
#[tauri::command]
pub async fn git_apply_hunk(
    state: State<'_, AppState>,
    path: String,
    file: String,
    hunk_index: usize,
    action: String,
) -> Result<(), CommandError> {
    if !state.rate_limiter.check("git_apply_hunk") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let action = match action.as_str() {
        "stage" => athena_git::HunkAction::Stage,
        "discard" => athena_git::HunkAction::Discard,
        other => {
            return Err(CommandError::InvalidInput(format!(
                "unknown hunk action: {other} (expected 'stage' or 'discard')"
            )))
        }
    };
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || {
        athena_git::apply_hunk(&root, &file, hunk_index, action).map_err(map_git_err)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("git apply hunk task failed: {e}")))?
}

/// File-level stage (`stage = true`) or discard of one file's unstaged
/// changes (discard restores the index version or deletes an untracked file).
#[tauri::command]
pub async fn git_apply_file(
    state: State<'_, AppState>,
    path: String,
    file: String,
    stage: bool,
) -> Result<(), CommandError> {
    if !state.rate_limiter.check("git_apply_file") {
        return Err(CommandError::InvalidInput(
            "Rate limit exceeded. Please wait a moment.".to_string(),
        ));
    }
    let root = validated_repo_root(&state, &path)?;
    tokio::task::spawn_blocking(move || {
        athena_git::apply_file(&root, &file, stage).map_err(map_git_err)
    })
    .await
    .map_err(|e| CommandError::Internal(format!("git apply file task failed: {e}")))?
}
