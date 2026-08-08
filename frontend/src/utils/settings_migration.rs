//! Persisted settings migrations used during frontend startup.

/// Migrate old separate pane-title settings into the unified `smart_pane_titles` key.
pub async fn migrate_smart_pane_titles() -> bool {
    if let Ok(v) = crate::tauri_bridge::store_get("smart_pane_titles").await {
        return v == "true";
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
