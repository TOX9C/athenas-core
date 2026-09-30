//! Git store: per-workspace repository status snapshots, loaded on demand
//! through the `git_status` / `git_diff` Tauri commands (backed by
//! `athena-git`). Diffs are too large to cache here — callers load them
//! per file selection via [`load_diff`].

use dioxus::prelude::*;
use std::collections::HashMap;

#[path = "git_model.rs"]
mod git_model;

pub use git_model::{DiffSet, FileDiff, FileStatus, Hunk, RepoStatus, StatusKind};
use git_model::MAX_TRACKED_REPOS;

use crate::tauri_bridge::{git_diff, git_discover, git_status};

/// Map of repository root → latest status snapshot.
pub type GitStore = Signal<HashMap<String, RepoStatus>>;

pub fn provide_git_store() {
    use_context_provider(|| Signal::new(HashMap::<String, RepoStatus>::new()));
}

pub fn use_git_store() -> GitStore {
    use_context::<GitStore>()
}

/// Discover the repo containing `path`; on success, refresh its status into
/// the store and return the repo root. Returns `None` outside a repository.
pub async fn refresh_status(mut store: GitStore, path: &str) -> Option<String> {
    let root = match git_discover(path).await {
        Ok(Some(root)) => root,
        _ => return None,
    };
    match git_status(&root).await {
        Ok(status) => {
            let mut guard = store.write();
            guard.insert(root.clone(), status);
            // Bounded-store invariant: evict the oldest insertion order
            // entry beyond the cap. BTreeMap ordering is sorted, not
            // insertion-ordered, so track an epoch per entry instead.
            // ponytail: alphabetical eviction, fine for 16-repo cap; upgrade
            // to true LRU if workspace counts grow.
            while guard.len() > MAX_TRACKED_REPOS {
                if let Some(first) = guard.keys().next().cloned() {
                    guard.remove(&first);
                }
            }
            Some(root)
        }
        Err(e) => {
            web_sys::console::error_1(&format!("git_status failed: {e:?}").into());
            None
        }
    }
}

/// Load a diff for the repo containing `path` (not stored; rendered directly
/// by the caller). `staged = true` diffs index vs HEAD.
pub async fn load_diff(path: &str, staged: bool) -> Option<DiffSet> {
    git_diff(path, staged).await.ok()
}
