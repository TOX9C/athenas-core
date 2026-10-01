//! Changes panel: unified diff review for the active workspace (or a selected
//! swarm agent's worktree). Per-hunk accept (stage) / discard and a
//! comment-to-agent channel are backed by `git_apply_hunk` / `git_apply_file`.
//!
//! Views: "Unstaged" (workdir vs index, hunk ops enabled — accepts stage the
//! hunk, discards revert it) and "Staged" (index vs HEAD, read-only).

use dioxus::prelude::*;

use crate::stores::git::{DiffLine, FileDiff, FileStatus};
use crate::stores::swarm::use_swarm_store;
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge;

#[component]
pub fn DiffPanel() -> Element {
    let workspace = use_workspace_store();
    let swarm_state = use_swarm_store();

    let mut staged_view = use_signal(|| false);
    let mut files = use_signal(Vec::<FileStatus>::new);
    let mut repo_root = use_signal(|| None::<String>);
    let mut selected_file = use_signal(|| None::<String>);
    let mut file_diff = use_signal(|| None::<Option<FileDiff>>);
    let mut comment_hunk = use_signal(|| None::<usize>);
    let mut checkpoints = use_signal(Vec::<crate::stores::git::Checkpoint>::new);
    let mut comment_text = use_signal(String::new);
    let mut notice = use_signal(|| None::<String>);

    // Review target: the active space dir, or a swarm agent's worktree.
    let targets: Vec<(String, String)> = {
        let ws = workspace.read();
        let swarm = swarm_state.read().active_swarm.clone();
        let active_dir = ws
            .active_space_id
            .as_ref()
            .and_then(|id| ws.spaces.iter().find(|s| &s.id == id))
            .map(|s| s.dir.clone());
        let mut list = Vec::new();
        if let Some(dir) = active_dir {
            list.push((dir.clone(), "Workspace".to_string()));
            if let Some(swarm) = swarm {
                if swarm.workspace_dir == dir {
                    for agent in &swarm.agents {
                        if let Some(wt) = &agent.worktree_path {
                            list.push((wt.clone(), format!("{}·{}", agent.role, agent.agent_type)));
                        }
                    }
                }
            }
        }
        list
    };
    let mut target_override = use_signal(|| None::<String>);
    let target = target_override
        .read()
        .clone()
        .or_else(|| targets.first().map(|(dir, _)| dir.clone()))
        .unwrap_or_default();

    // Reload file list + selected file diff whenever target/staged changes.
    // NOTE: all signal reads happen *inside* the effect so Dioxus tracks
    // them as dependencies (re-runs on target/switch/staged selection).
    use_effect(move || {
        let dir = target_override
            .read()
            .clone()
            .or_else(|| {
                let ws = workspace.read();
                ws.active_space_id
                    .as_ref()
                    .and_then(|id| ws.spaces.iter().find(|s| &s.id == id))
                    .map(|s| s.dir.clone())
            })
            .unwrap_or_default();
        let staged = staged_view();
        if dir.is_empty() {
            files.set(Vec::new());
            repo_root.set(None);
            return;
        }
        // Reset selection when the target switches; keep it for staged toggles.
        wasm_bindgen_futures::spawn_local(async move {
            match tauri_bridge::git_status(&dir).await {
                Ok(status) => {
                    repo_root.set(Some(status.root));
                    files.set(status.files);
                    notice.set(None);
                }
                Err(_) => {
                    repo_root.set(None);
                    files.set(Vec::new());
                    file_diff.set(None);
                }
            }
            if let Some(file) = selected_file.read().clone() {
                match tauri_bridge::git_diff_file(&dir, staged, &file).await {
                    Ok(set) => file_diff.set(Some(set.files.into_iter().next())),
                    Err(e) => notice.set(Some(format!("diff failed: {e:?}"))),
                }
            } else {
                file_diff.set(None);
            }
            if let Ok(list) = tauri_bridge::git_checkpoint_list(&dir).await {
                checkpoints.set(list);
            }
        });
    });


    let view = staged_view();
    let current_diff = file_diff.read().clone().flatten();
    let targets_snapshot = targets.clone();

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; overflow: hidden;",

            // Header: target picker + view toggle + refresh.
            div { style: "display: flex; align-items: center; gap: 6px; padding: 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                select {
                    style: "flex: 1; min-width: 0; font-family: var(--font-ui); font-size: 11px; color: var(--text); background: var(--bgTertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 3px 6px;",
                    onchange: move |e| {
                        target_override.set(Some(e.value()));
                        selected_file.set(None);
                        comment_hunk.set(None);
                    },
                    for (dir, label) in targets_snapshot.iter() {
                        option {
                            value: "{dir}",
                            selected: Some(dir.as_str()) == Some(target.as_str()),
                            "{label} — {dir}"
                        }
                    }
                }
                button {
                    class: "btn-ghost",
                    style: "font-size: 10px; padding: 3px 8px; border-radius: var(--radius-sm);",
                    onclick: move |_| staged_view.set(false),
                    disabled: !view,
                    "Unstaged"
                }
                button {
                    class: "btn-ghost",
                    style: "font-size: 10px; padding: 3px 8px; border-radius: var(--radius-sm);",
                    onclick: move |_| staged_view.set(true),
                    disabled: view,
                    "Staged"
                }
                button {
                    class: "btn-ghost",
                    style: "font-size: 10px; padding: 3px 8px; border-radius: var(--radius-sm);",
                    title: "Refresh diff",
                    onclick: move |_| reload_panel(target_override.read().clone(), staged_view(), selected_file.read().clone(), files, file_diff),
                    "↻"
                }
            }

            if repo_root.read().is_none() && !target.is_empty() {
                div { style: "padding: 16px; font-size: 11px; color: var(--textMuted);",
                    "Not a git repository (or the repository is outside the trusted roots)."
                }
            }

            if repo_root.read().is_some() {
                div { style: "padding: 6px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                    div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 4px;",
                        span { style: "font-family: var(--font-display); font-size: 10px; font-weight: 600; letter-spacing: 0.06em; color: var(--accent);", "CHECKPOINTS" }
                        button {
                            class: "btn-ghost",
                            style: "font-size: 10px; padding: 2px 8px;",
                            title: "Snapshot the current uncommitted state",
                            onclick: {
                                let dir = target.clone();
                                move |_| {
                                    let dir = dir.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let _ = tauri_bridge::git_checkpoint_create(&dir, "manual").await;
                                        if let Ok(list) = tauri_bridge::git_checkpoint_list(&dir).await {
                                            checkpoints.set(list);
                                        }
                                    });
                                }
                            },
                            "+ Create"
                        }
                    }
                    for cp in checkpoints.read().iter() {
                        {
                            let cp_id = cp.id.clone();
                            let label = if cp.label.is_empty() { "(unlabeled)" } else { &cp.label };
                            rsx! {
                                div { key: "{cp_id}", style: "display: flex; align-items: center; gap: 6px; padding: 3px 0; font-size: 10px; color: var(--textMuted);",
                                    span { style: "flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", title: "{cp_id}",
                                        "{label} · {format_ts(cp.created_at)}"
                                    }
                                    button {
                                        class: "btn-ghost",
                                        style: "font-size: 9px; padding: 1px 6px; color: var(--warning);",
                                        title: "Reset workdir + index to this snapshot (HEAD untouched)",
                                        onclick: {
                                            let dir = target.clone();
                                            let cp_id = cp_id.clone();
                                            move |_| {
                                                let dir = dir.clone();
                                                let cp_id = cp_id.clone();
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    match tauri_bridge::git_checkpoint_restore(&dir, &cp_id).await {
                                                        Ok(store_restored) => {
                                                            if store_restored {
                                                                notice.set(Some("Workspace restored. The app-store snapshot applies on next launch.".to_string()));
                                                            } else {
                                                                notice.set(Some("Workspace restored.".to_string()));
                                                            }
                                                            reload_panel(target_override.read().clone(), staged_view(), selected_file.read().clone(), files, file_diff);
                                                        }
                                                        Err(e) => notice.set(Some(format!("restore failed: {e:?}"))),
                                                    }
                                                });
                                            }
                                        },
                                        "Restore"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(msg) = notice.read().clone() {
                div { style: "padding: 8px; font-size: 11px; color: var(--error); border-bottom: 1px solid var(--border);", "{msg}" }
            }

            div { style: "flex: 1; display: flex; min-height: 0;",
                // File list (left column).
                div { style: "width: 40%; min-width: 140px; border-right: 1px solid var(--border); overflow-y: auto; flex-shrink: 0;",
                    for file in files.read().iter() {
                        {
                            let path = file.path.clone();
                            let is_sel = selected_file.read().as_deref() == Some(path.as_str());
                            rsx! {
                                button {
                                    key: "{path}",
                                    style: if is_sel {
                                        "display: flex; flex-direction: column; align-items: flex-start; gap: 2px; width: 100%; padding: 6px 8px; border: none; border-left: 2px solid var(--accent); background: var(--accentSubtle); cursor: pointer; text-align: left;"
                                    } else {
                                        "display: flex; flex-direction: column; align-items: flex-start; gap: 2px; width: 100%; padding: 6px 8px; border: none; border-left: 2px solid transparent; background: transparent; cursor: pointer; text-align: left;"
                                    },
                                    onclick: {
                                        let path = path.clone();
                                        move |_| {
                                            comment_hunk.set(None);
                                            selected_file.set(Some(path.clone()));
                                        }
                                    },
                                    span { style: "font-family: var(--font-mono); font-size: 11px; color: var(--text); word-break: break-all;", "{path}" }
                                    span { style: "font-size: 9px; color: var(--textMuted);",
                                        {status_badges(file)}
                                    }
                                }
                            }
                        }
                    }
                    if files.read().is_empty() && repo_root.read().is_some() {
                        div { style: "padding: 12px 8px; font-size: 11px; color: var(--textMuted);", "Working tree clean." }
                    }
                }

                // Diff body (right column).
                div { style: "flex: 1; min-width: 0; overflow-y: auto; overflow-x: auto;",
                    if let Some(diff) = current_diff.clone() {
                        div {
                            if diff.is_binary {
                                div { style: "padding: 12px; font-size: 11px; color: var(--textMuted);", "Binary file — accept/discard at file level only." }
                            }
                            div { style: "display: flex; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",
                                if !view {
                                    button {
                                        class: "btn-ghost",
                                        style: "font-size: 10px; padding: 3px 8px;",
                                        onclick: {
                                            let dir = target.clone();
                                            let file = diff.path.clone();
                                            move |_| {
                                                let dir = dir.clone();
                                                let file = file.clone();
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    let _ = tauri_bridge::git_apply_file(&dir, &file, true).await;
                                                    reload_panel(target_override.read().clone(), staged_view(), selected_file.read().clone(), files, file_diff);
                                                });
                                            }
                                        },
                                        "Accept file"
                                    }
                                    button {
                                        class: "btn-ghost",
                                        style: "font-size: 10px; padding: 3px 8px; color: var(--error);",
                                        onclick: {
                                            let dir = target.clone();
                                            let file = diff.path.clone();
                                            move |_| {
                                                let dir = dir.clone();
                                                let file = file.clone();
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    let _ = tauri_bridge::git_apply_file(&dir, &file, false).await;
                                                    reload_panel(target_override.read().clone(), staged_view(), selected_file.read().clone(), files, file_diff);
                                                });
                                            }
                                        },
                                        "Discard file"
                                    }
                                } else {
                                    span { style: "font-size: 10px; color: var(--textMuted); padding: 3px 8px;", "Staged view is read-only" }
                                }
                            }

                            for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
                                div { style: "border-bottom: 1px solid var(--border);",
                                    div { style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; background: var(--bgTertiary);",
                                        span { style: "font-family: var(--font-mono); font-size: 10px; color: var(--textMuted); flex: 1;", "{hunk.header}" }
                                        if !view && !diff.is_binary {
                                            button {
                                                class: "btn-ghost",
                                                style: "font-size: 10px; padding: 2px 6px;",
                                                title: "Stage this hunk (part of the file only)",
                                                onclick: {
                                                    let dir = target.clone();
                                                    let file = diff.path.clone();
                                                    move |_| {
                                                        let dir = dir.clone();
                                                        let file = file.clone();
                                                        wasm_bindgen_futures::spawn_local(async move {
                                                            let _ = tauri_bridge::git_apply_hunk(&dir, &file, hunk_idx, "stage").await;
                                                            reload_panel(target_override.read().clone(), staged_view(), selected_file.read().clone(), files, file_diff);
                                                        });
                                                    }
                                                },
                                                "Accept"
                                            }
                                            button {
                                                class: "btn-ghost",
                                                style: "font-size: 10px; padding: 2px 6px; color: var(--error);",
                                                title: "Revert this hunk in the working tree",
                                                onclick: {
                                                    let dir = target.clone();
                                                    let file = diff.path.clone();
                                                    move |_| {
                                                        let dir = dir.clone();
                                                        let file = file.clone();
                                                        wasm_bindgen_futures::spawn_local(async move {
                                                            let _ = tauri_bridge::git_apply_hunk(&dir, &file, hunk_idx, "discard").await;
                                                            reload_panel(target_override.read().clone(), staged_view(), selected_file.read().clone(), files, file_diff);
                                                        });
                                                    }
                                                },
                                                "Discard"
                                            }
                                            button {
                                                class: "btn-ghost",
                                                style: "font-size: 10px; padding: 2px 6px;",
                                                onclick: {
                                                    let idx = hunk_idx;
                                                    move |_| {
                                                        let cur = comment_hunk();
                                                        comment_hunk.set(if cur == Some(idx) { None } else { Some(idx) });
                                                    }
                                                },
                                                "Comment"
                                            }
                                        }
                                    }
                                    pre { style: "margin: 0; padding: 4px 8px; font-family: var(--font-mono); font-size: 10px; line-height: 1.5; white-space: pre; overflow-x: auto;",
                                        for line in hunk.lines.iter() {
                                            {render_diff_line(line)}
                                        }
                                    }
                                    if comment_hunk() == Some(hunk_idx) {
                                        div { style: "display: flex; gap: 6px; padding: 6px 8px; border-top: 1px dashed var(--border);",
                                            input {
                                                style: "flex: 1; min-width: 0; font-size: 11px; padding: 4px 6px; background: var(--bgTertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text);",
                                                placeholder: "Comment on this hunk — appended to the agent pane…",
                                                value: "{comment_text}",
                                                oninput: move |e| comment_text.set(e.value()),
                                            }
                                            button {
                                                class: "btn-primary",
                                                style: "font-size: 10px; padding: 3px 10px;",
                                                onclick: {
                                                    let dir = target.clone();
                                                    let file = diff.path.clone();
                                                    let header = hunk.header.clone();
                                                    move |_| {
                                                        let text = comment_text.read().trim().to_string();
                                                        if text.is_empty() { return; }
                                                        let dir = dir.clone();
                                                        let file = file.clone();
                                                        let header = header.clone();
                                                        wasm_bindgen_futures::spawn_local(async move {
                                                            let msg = format!(
                                                                "[review] {file} {header}: {text}\n"
                                                            );
                                                            match agent_pane_for(&dir).await {
                                                                Some(pane_id) => {
                                                                    match tauri_bridge::pty_write(&pane_id, &msg).await {
                                                                        Ok(()) => {
                                                                            comment_text.set(String::new());
                                                                            comment_hunk.set(None);
                                                                            notice.set(Some(format!("Sent to {pane_id}")));
                                                                        }
                                                                        Err(e) => notice.set(Some(format!("PTY write failed: {e:?}"))),
                                                                    }
                                                                }
                                                                None => notice.set(Some("No agent pane found for this directory.".to_string())),
                                                            }
                                                        });
                                                    }
                                                },
                                                "Send"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { style: "padding: 16px; font-size: 11px; color: var(--textMuted);",
                            "Select a file on the left to review its diff."
                        }
                    }
                }
            }
        }
    }
}

/// Reload status + selected file diff (Signals are Copy; reads the latest
/// values at call time so accept/discard buttons never refresh stale state).
fn reload_panel(
    dir: Option<String>,
    staged: bool,
    selected: Option<String>,
    mut files: Signal<Vec<FileStatus>>,
    mut file_diff: Signal<Option<Option<FileDiff>>>,
) {
    let Some(dir) = dir else { return };
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(status) = tauri_bridge::git_status(&dir).await {
            files.set(status.files);
        }
        if let Some(file) = selected {
            if let Ok(set) = tauri_bridge::git_diff_file(&dir, staged, &file).await {
                file_diff.set(Some(set.files.into_iter().next()));
            }
        }
    });
}

/// "S staged" / "U modified" style badges for the file list.
fn status_badges(file: &FileStatus) -> String {
    use crate::stores::git::StatusKind;
    let label = |k: Option<StatusKind>| -> Option<&'static str> {
        Some(match k? {
            StatusKind::New => "new",
            StatusKind::Modified => "modified",
            StatusKind::Deleted => "deleted",
            StatusKind::TypeChanged => "typechange",
            StatusKind::Renamed => "renamed",
            StatusKind::Conflicted => "conflicted",
        })
    };
    let mut out = String::new();
    if let Some(l) = label(file.index) {
        out.push_str(&format!("staged:{l} "));
    }
    if let Some(l) = label(file.workdir) {
        out.push_str(&format!("unstaged:{l}"));
    }
    out.trim().to_string()
}

/// One diff body line.
fn render_diff_line(line: &DiffLine) -> Element {
    let (fg, bg) = match line.origin {
        '+' => ("var(--success)", "rgba(74, 222, 128, 0.08)"),
        '-' => ("var(--error)", "rgba(248, 113, 113, 0.08)"),
        _ => ("var(--textMuted)", "transparent"),
    };
    rsx! {
        span { style: "display: inline; color: {fg}; background: {bg};", "{line.content}" }
    }
}

/// Route a hunk comment to the owning agent's PTY: the swarm agent whose
/// worktree is the target dir, falling back to the active space's first
/// agent pane.
async fn agent_pane_for(dir: &str) -> Option<String> {
    // Needs stores read on main app — this runs on the Dioxus thread already.
    let swarm = crate::stores::swarm::use_swarm_store();
    let workspace = crate::stores::workspace::use_workspace_store();
    if let Some(sw) = swarm.read().active_swarm.clone() {
        for agent in &sw.agents {
            if agent.worktree_path.as_deref() == Some(dir) && !agent.pane_id.is_empty() {
                return Some(agent.pane_id.clone());
            }
        }
    }
    let ws = workspace.read();
    let space = ws
        .active_space_id
        .as_ref()
        .and_then(|id| ws.spaces.iter().find(|s| &s.id == id))?;
    space
        .panes
        .iter()
        .find(|p| !matches!(p.agent_type, crate::types::workspace::AgentType::Shell))
        .map(|p| p.id.clone())
}

/// Compact absolute time for a checkpoint row.
fn format_ts(secs: i64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((secs.max(0) * 1000) as f64));
    format!(
        "{:02}-{:02} {:02}:{:02}",
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes()
    )
}
