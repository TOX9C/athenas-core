//! Persisted settings migrations used during frontend startup.

/// Migrate old separate pane-title settings into the unified `smart_pane_titles` key.
pub async fn migrate_smart_pane_titles() -> bool {
    if let Ok(v) = crate::tauri_bridge::store_get("smart_pane_titles").await {
        // An empty/whitespace value means the key exists but was never really
        // set — treat it as absent and fall through to the legacy merge.
        let v = v.trim();
        if !v.is_empty() {
            return v == "true";
        }
    }
    let auto_gen = crate::tauri_bridge::store_get("auto_generate_titles")
        .await
        .map(|v| v == "true")
        .unwrap_or(true);
    let summarize = crate::tauri_bridge::store_get("summarize_agent_titles")
        .await
        .map(|v| v == "true")
        .unwrap_or(false);
    let merged = auto_gen || summarize;
    let _ =
        crate::tauri_bridge::store_set("smart_pane_titles", if merged { "true" } else { "false" })
            .await;
    merged
}

/// Load the swarm worktree cleanup preference (defaults to true so teardown
/// never leaks worktrees for users who never open settings).
pub async fn migrate_swarm_cleanup_worktrees() -> bool {
    crate::tauri_bridge::store_get("swarm_cleanup_worktrees")
        .await
        .map(|v| v.trim() == "true" || v.trim().is_empty())
        .unwrap_or(true)
}
