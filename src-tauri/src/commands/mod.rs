/// Minimal HTML escape to prevent XSS in contexts that might render HTML.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

mod agent_notify;
mod agents;
mod athena;
mod browser;
pub mod caps;
mod diagnostics;
mod drop;
mod filesystem;
mod kanban;
mod mcp;
mod notification;
mod output;
mod plan;
mod plugin;
mod provider_config;
mod pty;
pub(crate) mod relay;
mod resume;
mod search;
mod session;
mod shell;
pub(crate) mod store;
mod swarm;
pub(crate) mod voice;
mod window;
mod workspace;

pub use agent_notify::agent_notify_install;
pub use agents::{
    agent_cancel_input, agent_comms_send, agent_comms_sessions, agent_comms_token,
    agent_disconnect, agent_get_status, agent_get_token, agent_respond_input, agent_send_message,
    agents_list,
};
#[cfg(test)]
pub(crate) use athena::prompt_is_sensitive;
pub use athena::{
    athena_cancel_stream, athena_chat, athena_chat_stream, athena_chat_with_images,
    athena_chat_with_session, athena_clear_context, athena_set_session_context, athena_user_answer,
    clear_api_key, store_api_key, summarize_agent_title,
};
pub use browser::{
    browser_back, browser_forward, browser_hide, browser_navigate, browser_reload,
    browser_set_bounds, browser_show, shutdown_browser_children,
};
pub use diagnostics::diagnostics_export;
pub use drop::pty_stage_drop_file;
pub use filesystem::{
    fs_exists, fs_list_dir, fs_read_file, fs_read_file_as_base64, fs_search_files,
    fs_show_image_dialog, fs_show_open_dialog, fs_write_file,
};
pub use kanban::{kanban_create_task, kanban_delete_task, kanban_get_tasks, kanban_update_task};
pub use mcp::{mcp_broadcast, mcp_handle_request, mcp_init, mcp_shutdown, mcp_tools};
pub use notification::{
    notification_clear_all, notification_count, notification_counts, notification_dismiss,
    notification_history, notification_mark_all_read, notification_mark_read, notification_push,
    notification_resolve,
};
pub use output::{
    get_pane_history, output_buffer_append, output_buffer_clear, output_buffer_get,
    output_buffer_list,
};
pub use plan::{plan_create, plan_get, plan_update_step};
pub use plugin::{
    plugin_disable, plugin_enable, plugin_get, plugin_get_config, plugin_host_discover_plugins,
    plugin_host_emit_event, plugin_host_get_session, plugin_host_list_sessions,
    plugin_host_remove_plugin, plugin_host_setup_plugin, plugin_host_subscribe,
    plugin_host_unregister_session, plugin_host_update_status, plugin_list, plugin_register,
    plugin_set_config, plugin_set_error, plugin_unregister,
};
pub use provider_config::llm_list_models;
pub(crate) use pty::{
    now_ms, pty_attach_listener_relay, pty_read_loop, session_foreground_label, RelayReplayStore,
};
pub use pty::{
    pty_agent_info, pty_attach_listener, pty_default_shell, pty_detach_listener,
    pty_foreground_process, pty_get_cwd, pty_get_history, pty_has_session, pty_is_ready, pty_kill,
    pty_raw_replay, pty_resize, pty_set_xterm, pty_spawn, pty_spawn_agent, pty_write,
    read_clipboard_text,
};
pub use relay::{
    relay_list_shared_panes, relay_pairing_respond, relay_request_pane_share,
    relay_set_pane_shared, relay_start, relay_status, relay_stop,
};
pub use relay::{relay_token, RELAY_ENABLED_KEY};
pub use resume::capture_resume_ids_on_exit;
#[cfg(test)]
pub(crate) use resume::merge_resume_ids_into_workspaces;
pub use search::{search_code, search_ripgrep};
pub use session::{
    session_add_message, session_create, session_delete, session_get, session_list, session_update,
};
pub use shell::{
    shell_integration_compatible, shell_integration_parse, shell_integration_script,
    shell_integration_strip,
};
pub use store::{store_delete, store_get, store_has, store_set, test_llm_api_key};
pub use swarm::{
    swarm_create, swarm_create_task, swarm_read_mailbox, swarm_read_state, swarm_send_message,
    swarm_set_status, swarm_start_watch, swarm_stop_watch, swarm_update_agent, swarm_update_task,
};
pub use voice::{voice_record_start, voice_record_stop};
pub use window::{
    window_close, window_is_maximized, window_maximize, window_minimize, window_platform,
};
pub use workspace::{
    workspace_add_trusted_root, workspace_list_trusted_roots, workspace_remove_trusted_root,
};

// ── Path validation helpers ──────────────────────────────────────────────────

/// Get the canonicalized workspace root for path sandboxing.
///
/// The workspace root is the *project* directory — the ancestor that contains
/// `src-tauri/` — not the process's current working directory, which is
/// launch-context-dependent:
///   - `cargo tauri dev` runs the backend with cwd = `src-tauri/`.
///   - A bundled release `.app` launched from Finder has cwd = `/`.
///     Using `current_dir()` directly made the sandbox root `src-tauri/` in dev
///     (so the real project root one level up was wrongly rejected) and `/` in
///     release (so the sandbox silently allowed every path — a latent hole, not a
///     correct config).
///
/// Resolution first looks for the project-root marker
/// (`src-tauri/tauri.conf.json`) from both `current_dir()` and the executable
/// path. Finder-launched production apps do not contain that source marker, so
/// the signed bundle's `Contents/Resources` directory is the only packaged
/// fallback accepted. Any other unknown layout fails closed.
fn get_workspace_root() -> Result<std::path::PathBuf, CommandError> {
    let raw = std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("Failed to get workspace root: {}", e)))?;

    let exe = std::env::current_exe()
        .map_err(|e| CommandError::Internal(format!("Failed to get current exe: {}", e)))?;

    // Candidate starting points for the upward marker walk. `raw` wins in dev;
    // `exe` wins for a Finder-launched release bundle whose cwd is `/`.
    let starts = vec![raw.clone(), exe.clone()];
    let resolved = resolve_workspace_root(&starts).or_else(|_| {
        bundled_resource_root(&exe).ok_or_else(|| {
            CommandError::Internal(
                "Cannot locate workspace root: no project marker or app Resources directory".into(),
            )
        })
    })?;
    let canon = resolved.canonicalize().map_err(|e| {
        CommandError::Internal(format!("Failed to canonicalize workspace root: {}", e))
    })?;
    log::debug!(
        "[workspace_root] current_dir={:?} exe={:?} resolved={:?} canonicalized={:?}",
        raw,
        exe,
        resolved,
        canon
    );
    Ok(canon)
}

/// Resolve the project root from candidate starting points.
///
/// This helper deliberately has no fallback root. A missing marker is an
/// authorization failure, not a reason to broaden filesystem access.
fn resolve_workspace_root(
    starts: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, CommandError> {
    let marker_name = std::path::Path::new("tauri.conf.json");
    let src_tauri = std::path::Path::new("src-tauri");

    for start in starts {
        let mut dir = start.as_path();
        loop {
            if dir.join(src_tauri).join(marker_name).exists() {
                return Ok(dir.to_path_buf());
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    Err(CommandError::Internal(
        "Cannot locate workspace root: src-tauri/tauri.conf.json not found".into(),
    ))
}

/// Return the app's packaged resource directory for a standard macOS bundle.
///
/// The strict `Contents/MacOS/<executable>` shape prevents arbitrary
/// executable directories from becoming filesystem roots in development or
/// malformed launch contexts.
fn bundled_resource_root(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let macos = exe.parent()?;
    if macos.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let resources = contents.join("Resources");
    resources.is_dir().then_some(resources)
}

/// Key under which the user's trusted workspace roots are persisted, as a
/// JSON array of canonicalized absolute path strings.
///
/// Athena is a *multi-project* terminal launcher: every Space carries an
/// arbitrary working directory (`types::workspace::Space::dir`), and the whole
/// point is to run terminals and AI agents in user-chosen project folders.
/// The sandbox below therefore accepts any path descending from the app's own
/// project root *or* any trusted root added here. A root is added the moment
/// the user deliberately creates a Space for it — the authorization gesture.
const TRUSTED_ROOTS_KEY: &str = "workspace.trusted_roots";

/// Load the user's trusted workspace roots from the persistent store.
///
/// Each stored entry is re-canonicalized on load so that comparisons against a
/// canonicalized request path stay stable. Roots that no longer resolve (the
/// directory was moved/deleted/renamed) are silently skipped — they simply
/// can't authorize anything until re-added. A malformed or missing key yields
/// an empty list (first run, or a corrupt value); the store is never trusted
/// to hand back a canonicalized form.
fn load_trusted_roots(store: &athena_store::KeyValueStore) -> Vec<std::path::PathBuf> {
    let raw: Option<Vec<String>> = match store.get(TRUSTED_ROOTS_KEY) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[trusted_roots] failed to read key '{}': {}",
                TRUSTED_ROOTS_KEY,
                e
            );
            return Vec::new();
        }
    };
    match raw {
        Some(list) => list
            .into_iter()
            .filter_map(|p| {
                std::path::PathBuf::from(&p)
                    .canonicalize()
                    .map_err(|e| {
                        log::debug!(
                            "[trusted_roots] skipping '{}': canonicalize failed: {}",
                            p,
                            e
                        );
                        e
                    })
                    .ok()
            })
            .collect(),
        None => Vec::new(),
    }
}

/// Log a one-time startup diagnostic explaining why macOS may re-prompt for
/// "Files and Folders" (Documents / Desktop / Downloads) permissions.
///
/// macOS keys these grants to the app's code signature. Locally-built bundles
/// are ad-hoc signed (`tauri build` without `APPLE_SIGNING_IDENTITY`), and an
/// ad-hoc signature changes on every rebuild, so macOS treats each build as a
/// new app and re-prompts. That is a signing/distribution property, not a bug
/// in the file-access code. Surfacing it here makes the symptom diagnosable
/// from the app's own logs. Local builds should use a stable signing identity
/// when macOS permission persistence matters.
#[cfg(target_os = "macos")]
pub(crate) fn log_macos_permission_diagnostics(store: &athena_store::KeyValueStore) {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "<unknown>".to_string());

    // A bare `target/debug/athenas-core` binary has no app-bundle identity for
    // macOS TCC to key a grant against; a `.app` running from a transient
    // (quarantined / DMG-mounted) location behaves the same way.
    let is_bundle = exe.contains("/Contents/MacOS/");

    let home = std::env::var("HOME").ok();
    let in_protected_folder = |p: &std::path::Path| -> bool {
        let Some(home) = home.as_deref() else {
            return false;
        };
        let base = std::path::Path::new(home);
        ["Documents", "Desktop", "Downloads"]
            .iter()
            .any(|d| p.starts_with(base.join(d)))
    };

    let mut protected: Vec<String> = Vec::new();
    if let Ok(root) = get_workspace_root() {
        if in_protected_folder(&root) {
            protected.push(root.to_string_lossy().into_owned());
        }
    }
    for root in load_trusted_roots(store) {
        if in_protected_folder(&root) {
            protected.push(root.to_string_lossy().into_owned());
        }
    }

    if !is_bundle {
        log::warn!(
            "[permissions] running outside an app bundle ('{}'); macOS cannot persist TCC grants for a bare executable",
            exe
        );
    }
    if !protected.is_empty() {
        log::warn!(
            "[permissions] workspace/trusted roots live in a macOS-protected folder (Documents/Desktop/Downloads): {:?}",
            protected
        );
        log::warn!(
            "[permissions] if this build is ad-hoc signed (check `codesign -dv`), the Files & Folders prompt reappears after every rebuild; sign with a stable Developer ID identity to persist the grant"
        );
    }
}

/// The full set of roots a request path may descend from: the app's own
/// project root (always implicitly trusted) plus every user-added trusted
/// root. All entries are canonicalized.
fn effective_roots(
    store: &athena_store::KeyValueStore,
) -> Result<Vec<std::path::PathBuf>, CommandError> {
    effective_roots_from(store, get_workspace_root())
}

fn effective_roots_from(
    store: &athena_store::KeyValueStore,
    workspace_root: Result<std::path::PathBuf, CommandError>,
) -> Result<Vec<std::path::PathBuf>, CommandError> {
    let mut roots = vec![workspace_root?];
    roots.extend(load_trusted_roots(store));
    Ok(roots)
}

/// True if `canonical` is equal to or a descendant of any root in `roots`.
///
/// `canonical` and every entry of `roots` must already be canonicalized
/// (symlinks resolved, no `..`) — which is exactly how the validators below
/// feed it. This preserves the existing traversal/symlink-escape guarantees;
/// we only widen the *set* of acceptable top-level directories, never the
/// canonicalization discipline. Exposed for unit testing.
fn is_within_any_root(canonical: &std::path::Path, roots: &[std::path::PathBuf]) -> bool {
    roots.iter().any(|r| canonical.starts_with(r))
}

/// Validate that a path exists, is inside the sandbox, and return its
/// canonical form.
///
/// The sandbox is the union of the app's project root and the user's trusted
/// workspace roots (see [`load_trusted_roots`]). The project root, every
/// trusted root, and the request path are all canonicalized before the
/// descendant check, so symlink escapes and `..` traversal are neutralized
/// exactly as before — only the set of permissible top-level directories
/// grows.
fn validate_path_exists(
    store: &athena_store::KeyValueStore,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, CommandError> {
    let roots = effective_roots(store)?;
    // Relative paths resolve against the project root (first effective root).
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots
            .first()
            .cloned()
            .ok_or_else(|| CommandError::Internal("workspace root is unavailable".into()))?
            .join(path)
    };
    let canonicalized = path.canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CommandError::NotFound("Path does not exist".to_string())
        } else {
            CommandError::Internal(format!("Failed to canonicalize path: {}", e))
        }
    })?;
    if !is_within_any_root(&canonicalized, &roots) {
        // Do NOT echo the canonicalized workspace root or the requested path
        // back to the frontend — it confirms on-disk layout (user home
        // path, project location) to a probing renderer. Generic message only.
        return Err(CommandError::PermissionDenied(
            "Path is outside the workspace".to_string(),
        ));
    }
    Ok(canonicalized)
}

/// Validate a path for write operations (creates parent dirs if needed).
///
/// Tolerates a not-yet-existing leaf (the file we're about to write) by
/// canonicalizing its parent and re-joining the file name, then applies the
/// same multi-root descendant check as [`validate_path_exists`].
fn validate_path(
    store: &athena_store::KeyValueStore,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, CommandError> {
    let roots = effective_roots(store)?;
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots
            .first()
            .cloned()
            .ok_or_else(|| CommandError::Internal("workspace root is unavailable".into()))?
            .join(path)
    };
    let canonical = if full_path.exists() {
        full_path
            .canonicalize()
            .map_err(|e| CommandError::Internal(format!("Failed to canonicalize path: {}", e)))?
    } else {
        let parent = full_path.parent().ok_or_else(|| {
            CommandError::InvalidInput(format!("path {:?} has no parent", full_path))
        })?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| CommandError::Internal(format!("Failed to canonicalize parent: {}", e)))?;
        match full_path.file_name() {
            Some(name) => canonical_parent.join(name),
            None => {
                return Err(CommandError::InvalidInput(format!(
                    "path {:?} has no file name",
                    full_path
                )))
            }
        }
    };
    if !is_within_any_root(&canonical, &roots) {
        return Err(CommandError::PermissionDenied(
            "Path is outside the workspace".to_string(),
        ));
    }
    if let Some(parent) = canonical.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CommandError::Internal(format!("Failed to create parent directories: {}", e))
        })?;
    }
    Ok(canonical)
}

// ── Structured error type for Tauri commands ────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // is_within_any_root is pure logic over canonicalized paths, so we can
    // exercise it without touching the disk (other than via canonicalize of
    // the temp dirs we construct).

    #[test]
    fn workspace_root_resolution_fails_closed_without_marker() {
        let missing_root = std::env::temp_dir().join("athena_missing_workspace_marker");
        std::fs::create_dir_all(&missing_root).unwrap();

        let result = resolve_workspace_root(std::slice::from_ref(&missing_root));

        assert!(
            matches!(result, Err(CommandError::Internal(message)) if message.contains("Cannot locate workspace root"))
        );
        assert!(bundled_resource_root(&missing_root.join("app")).is_none());
        std::fs::remove_dir_all(&missing_root).ok();
    }

    #[test]
    fn effective_roots_propagates_workspace_resolution_failure() {
        let store = athena_store::KeyValueStore::new_empty();
        let result = effective_roots_from(
            &store,
            Err(CommandError::Internal("workspace resolution failed".into())),
        );

        assert!(
            matches!(result, Err(CommandError::Internal(message)) if message == "workspace resolution failed")
        );
    }

    #[test]
    fn bundled_resource_root_accepts_only_standard_bundle_layout() {
        let root = std::env::temp_dir().join("athena_bundle_layout");
        let macos = root.join("Athena.app/Contents/MacOS");
        let resources = root.join("Athena.app/Contents/Resources");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::create_dir_all(&resources).unwrap();

        assert_eq!(
            bundled_resource_root(&macos.join("athena")),
            Some(resources.clone())
        );
        assert!(bundled_resource_root(&root.join("target/debug/athena")).is_none());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn is_within_any_root_accepts_descendant_of_a_trusted_root() {
        let temp = std::env::temp_dir();
        let sub = temp.join("athena_trusted_descendant");
        std::fs::create_dir_all(&sub).unwrap();
        let canon = sub.canonicalize().unwrap();
        let roots = vec![temp.canonicalize().unwrap()];
        assert!(is_within_any_root(&canon, &roots));
        std::fs::remove_dir_all(&sub).ok();
    }

    #[test]
    fn is_within_any_root_rejects_sibling_outside_all_roots() {
        // Two roots; a path under neither must be rejected.
        let temp = std::env::temp_dir();
        let a = temp.join("athena_tr_root_a");
        let b = temp.join("athena_tr_root_b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let canon_a = a.canonicalize().unwrap();
        let canon_b = b.canonicalize().unwrap();
        // b is NOT under a's tree
        let roots = vec![canon_a.clone()];
        assert!(!is_within_any_root(&canon_b, &roots));
        // but is accepted once b is itself a root
        let roots = vec![canon_a, canon_b.clone()];
        assert!(is_within_any_root(&canon_b, &roots));
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn load_trusted_roots_recanonicalizes_and_skips_missing() {
        let store = athena_store::KeyValueStore::new_empty();

        // A real, canonicalizable path round-trips; a missing/garbage entry
        // is skipped without error.
        let temp = std::env::temp_dir().join("athena_tr_load_real");
        std::fs::create_dir_all(&temp).unwrap();
        let canon = temp.canonicalize().unwrap().to_string_lossy().into_owned();
        store
            .set_sync(
                TRUSTED_ROOTS_KEY,
                &vec![
                    canon.clone(),
                    "/this/path/does/not/exist/athena".to_string(),
                ],
            )
            .unwrap();
        let roots = load_trusted_roots(&store);
        // only the real one survives
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], std::path::PathBuf::from(&canon));

        // Missing key -> empty list, not an error.
        let store2 = athena_store::KeyValueStore::new_empty();
        assert!(load_trusted_roots(&store2).is_empty());

        // Malformed value -> empty list, not an error.
        let store3 = athena_store::KeyValueStore::new_empty();
        store3.set_sync(TRUSTED_ROOTS_KEY, &"not-a-json-array").ok();
        assert!(load_trusted_roots(&store3).is_empty());

        std::fs::remove_dir_all(&temp).ok();
    }

    #[test]
    fn validate_path_exists_accepts_trusted_root_outside_project() {
        // Simulates the bug report: a Space dir outside the app's project root
        // must be accepted once trusted, and rejected before it's trusted.
        let store = athena_store::KeyValueStore::new_empty();
        let temp = std::env::temp_dir().join("athena_tr_outside_project");
        std::fs::create_dir_all(&temp).unwrap();
        let canon = temp.canonicalize().unwrap();

        // Before trusting: the result must match "is canon under the project
        // root" — robust to where the test physically runs.
        let pre = validate_path_exists(&store, &canon);
        let project_root = get_workspace_root().ok();
        let expected_ok = project_root
            .as_ref()
            .map(|r| canon.starts_with(r))
            .unwrap_or(false);
        assert_eq!(pre.is_ok(), expected_ok);

        // After trusting: accepted.
        let mut roots = load_trusted_roots(&store);
        roots.push(canon.clone());
        let strs: Vec<String> = roots
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        store.set_sync(TRUSTED_ROOTS_KEY, &strs).unwrap();
        assert!(validate_path_exists(&store, &canon).is_ok());

        std::fs::remove_dir_all(&temp).ok();
    }

    fn ids(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn merge_resume_ids_sets_resume_fields_on_matching_pane() {
        let store = athena_store::KeyValueStore::new_empty();
        // A persisted workspace whose pane previously had its banner dismissed.
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({
                    "spaces": [{
                        "id": "space-1",
                        "panes": [
                            { "id": "pane-a", "resume_id": "old", "resume_dismissed": true },
                            { "id": "pane-b" }
                        ]
                    }],
                    "active_space_id": "space-1"
                })
                .to_string(),
            )
            .unwrap();

        let updated = merge_resume_ids_into_workspaces(
            &store,
            &ids(&[("pane-a", "new-resume-id")]),
            &ids(&[]),
        )
        .unwrap();
        assert_eq!(updated, 1);

        let json: String = store.get("workspaces").unwrap().unwrap();
        let root: serde_json::Value = serde_json::from_str(&json).unwrap();
        let pane_a = &root["spaces"][0]["panes"][0];
        assert_eq!(pane_a["resume_id"], "new-resume-id");
        assert_eq!(pane_a["resume_dismissed"], false);
        // When no explicit resume_cmd is captured, the fallback is the
        // resume_id itself (so Shell panes can show the banner too).
        assert_eq!(pane_a["resume_cmd"], "new-resume-id");
        // Untouched pane keeps its shape (no resume id forced on it).
        let pane_b = &root["spaces"][0]["panes"][1];
        assert!(pane_b.get("resume_id").is_none());
    }

    #[test]
    fn merge_resume_ids_is_noop_when_no_pane_matches() {
        let store = athena_store::KeyValueStore::new_empty();
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({
                    "spaces": [{ "id": "s", "panes": [{ "id": "pane-x" }] }],
                    "active_space_id": "s"
                })
                .to_string(),
            )
            .unwrap();

        let updated =
            merge_resume_ids_into_workspaces(&store, &ids(&[("pane-unknown", "rid")]), &ids(&[]))
                .unwrap();
        assert_eq!(updated, 0);
    }

    #[test]
    fn merge_resume_ids_handles_missing_or_empty_workspaces_key() {
        let store = athena_store::KeyValueStore::new_empty();
        // Missing key.
        assert_eq!(
            merge_resume_ids_into_workspaces(&store, &ids(&[("p", "r")]), &ids(&[])).unwrap(),
            0
        );
        // Empty string value.
        store.set_sync("workspaces", &"").unwrap();
        assert_eq!(
            merge_resume_ids_into_workspaces(&store, &ids(&[("p", "r")]), &ids(&[])).unwrap(),
            0
        );
        // Empty ids map is a no-op even with a real workspace.
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({ "spaces": [], "active_space_id": null }).to_string(),
            )
            .unwrap();
        assert_eq!(
            merge_resume_ids_into_workspaces(&store, &ids(&[]), &ids(&[])).unwrap(),
            0
        );
    }
}

#[cfg(test)]
mod title_command_tests {
    use super::prompt_is_sensitive;

    #[test]
    fn sensitive_prompt_blocks_plaintext_variants() {
        let cases = [
            "my password is x",
            "set the API_KEY=..",
            "a secret token",
            "auth header here",
            "credential leak",
        ];
        for kw in cases {
            assert!(prompt_is_sensitive(kw), "expected sensitive: {kw}");
        }
    }

    #[test]
    fn sensitive_prompt_blocks_l33t_variants() {
        let cases = ["p@ssword", "t0k3n", "API_K3Y", "s3cret"];
        for kw in cases {
            assert!(prompt_is_sensitive(kw), "expected l33t-sensitive: {kw}");
        }
    }

    #[test]
    fn normal_prompt_passes_filter() {
        assert!(!prompt_is_sensitive("analyze the codebase"));
        assert!(!prompt_is_sensitive("what rust version is this"));
        assert!(!prompt_is_sensitive("hi"));
    }
}
