pub mod session;
pub mod store;
pub mod types;

// Re-export types for convenient access
pub use session::SessionStore;
pub use store::{FlushOutcome, KeyValueStore, StoreError};
pub use types::*;

/// Remove stale `.{name}.tmp-*` files left behind by a crash between
/// temp-file creation and the atomic rename (`atomic_write` /
/// `write_file_durable`). Called once at store open so orphans cannot
/// accumulate forever. Best-effort: failures to read/remove are ignored.
///
/// Only files unmodified for at least 60 seconds are removed: an in-flight
/// writer in another process creating its temp file right now must not have
/// it swept out from under the rename.
///
/// `name_prefix` restricts the sweep to temp files of one destination file
/// (e.g. `Some("store.json")` matches `.store.json.tmp-*`); `None` matches
/// any `.*.tmp-*` entry (safe in directories fully owned by this crate, such
/// as the session/image dirs).
pub(crate) fn sweep_orphaned_temp_files(dir: &std::path::Path, name_prefix: Option<&str>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let matches = match name_prefix {
            Some(prefix) => name.starts_with(&format!(".{prefix}.tmp-")),
            None => name.starts_with('.') && name.contains(".tmp-"),
        };
        if !matches || !entry.path().is_file() {
            continue;
        }
        let is_stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .map(|age| age >= std::time::Duration::from_secs(60))
            .unwrap_or(false);
        if is_stale && std::fs::remove_file(entry.path()).is_ok() {
            eprintln!(
                "athena-store: removed orphaned temp file {}",
                entry.path().display()
            );
        }
    }
}

#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;
