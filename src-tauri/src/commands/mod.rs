/// Minimal HTML escape to prevent XSS in contexts that might render HTML.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

mod agents;
mod athena;
mod browser;
pub mod caps;
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
mod store;
mod swarm;
mod window;
mod workspace;

pub use agents::{
    agent_cancel_input, agent_comms_send, agent_comms_sessions, agent_comms_token,
    agent_disconnect, agent_get_status, agent_get_token, agent_respond_input, agent_send_message,
    agents_list,
};
#[cfg(test)]
pub(crate) use athena::prompt_is_sensitive;
pub use athena::{
    athena_chat, athena_chat_with_images, athena_chat_with_session, athena_clear_context,
    athena_set_session_context, athena_user_answer, clear_api_key, store_api_key,
    summarize_agent_title,
};
pub use browser::{
    browser_back, browser_forward, browser_hide, browser_navigate, browser_reload,
    browser_set_bounds, browser_show,
};
pub use filesystem::{
    fs_exists, fs_list_dir, fs_read_file, fs_read_file_as_base64, fs_search_files,
    fs_show_image_dialog, fs_show_open_dialog, fs_write_file,
};
pub use kanban::{kanban_create_task, kanban_delete_task, kanban_get_tasks, kanban_update_task};
pub use mcp::{mcp_broadcast, mcp_handle_request, mcp_init, mcp_shutdown, mcp_tools};
pub use notification::{
    notification_clear_all, notification_count, notification_counts, notification_dismiss,
    notification_history, notification_mark_all_read, notification_mark_read, notification_push,
};
pub use output::{
    get_pane_history, output_buffer_append, output_buffer_clear, output_buffer_get, output_buffer_list,
};
pub use plan::{plan_create, plan_get, plan_update_step};
pub use plugin::{
    plugin_disable, plugin_enable, plugin_get, plugin_get_config, plugin_host_discover_plugins,
    plugin_host_emit_event, plugin_host_get_session, plugin_host_list_sessions,
    plugin_host_remove_plugin, plugin_host_setup_plugin, plugin_host_subscribe,
    plugin_host_unregister_session, plugin_host_update_status, plugin_list, plugin_register,
    plugin_set_config, plugin_set_error, plugin_unregister,
};
pub(crate) use pty::{now_ms, pty_read_loop, session_foreground_label};
pub use pty::{
    pty_agent_info, pty_attach_listener, pty_default_shell, pty_foreground_process, pty_get_cwd,
    pty_get_history, pty_has_session, pty_is_ready, pty_kill, pty_resize, pty_set_raw_paused,
    pty_set_xterm, pty_spawn, pty_spawn_agent, pty_write, read_clipboard_text,
};
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
pub use relay::{relay_start, relay_status, relay_stop};
pub use relay::{relay_token, RELAY_ENABLED_KEY};
pub use store::{store_delete, store_get, store_has, store_set, test_llm_api_key};
pub use swarm::{swarm_read_mailbox, swarm_read_state, swarm_send_message};
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
/// Resolution: look for the project-root marker (`src-tauri/tauri.conf.json`)
/// by walking up from both `current_dir()` *and* the executable's directory.
/// The exe path is stable across launch contexts: in dev it is
/// `target/debug/athenas-core`, in release `…/Athena's Core.app/Contents/MacOS/…`,
/// both of which live under the project root when built locally. If neither
/// walk finds the marker, fall back to `current_dir()` so behavior is no worse
/// than before (and so the validator still has *some* root to check against).
fn get_workspace_root() -> Result<std::path::PathBuf, CommandError> {
    let raw = std::env::current_dir()
        .map_err(|e| CommandError::Internal(format!("Failed to get workspace root: {}", e)))?;

    let exe = std::env::current_exe()
        .map_err(|e| CommandError::Internal(format!("Failed to get current exe: {}", e)))?;

    // Candidate starting points for the upward marker walk. `raw` (cwd)
    // wins in dev; `exe` wins for a Finder-launched release bundle whose
    // cwd is `/`. Both are cheap to try.
    let starts: Vec<std::path::PathBuf> = vec![raw.clone(), exe.clone()];

    let marker_name = std::path::Path::new("tauri.conf.json");
    let src_tauri = std::path::Path::new("src-tauri");

    let mut root_candidate: Option<std::path::PathBuf> = None;
    'outer: for start in &starts {
        let mut dir = start.as_path();
        loop {
            if dir.join(src_tauri).join(marker_name).exists() {
                root_candidate = Some(dir.to_path_buf());
                break 'outer;
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    let resolved = root_candidate.ok_or_else(|| {
        CommandError::Internal(
            "Cannot locate workspace root: src-tauri/tauri.conf.json not found".into(),
        )
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

/// The full set of roots a request path may descend from: the app's own
/// project root (always implicitly trusted) plus every user-added trusted
/// root. All entries are canonicalized.
fn effective_roots(store: &athena_store::KeyValueStore) -> Vec<std::path::PathBuf> {
    let mut roots = vec![get_workspace_root().unwrap_or_else(|_| std::path::PathBuf::from("/"))];
    roots.extend(load_trusted_roots(store));
    roots
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
    let roots = effective_roots(store);
    // Relative paths resolve against the project root (first effective root).
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots
            .first()
            .cloned()
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
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
    let roots = effective_roots(store);
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        roots
            .first()
            .cloned()
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
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
