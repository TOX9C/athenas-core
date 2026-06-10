use std::cell::RefCell;
use std::rc::Rc;

use super::file_tree_node::FileTreeNode;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::editor::{use_editor_store, EditorFile};
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge;
use dioxus::prelude::*;

/// Simple file tree node data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub is_expanded: bool,
}

/// Lightweight struct matching the Tauri fs_list_dir response shape.
#[derive(serde::Deserialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// Parse the JSON response from fs_list_dir into FileNode entries.
fn parse_dir_entries(response: &str) -> Vec<FileNode> {
    serde_json::from_str::<Vec<DirEntry>>(response)
        .unwrap_or_default()
        .into_iter()
        .map(|e| FileNode {
            name: e.name,
            path: e.path,
            is_dir: e.is_dir,
            children: Vec::new(),
            is_expanded: false,
        })
        .collect()
}

/// Detect language from file extension.
fn detect_language(path: &str) -> String {
    if let Some(ext) = path.rsplit('.').next() {
        match ext {
            "rs" => "rust".to_string(),
            "ts" | "tsx" => "typescript".to_string(),
            "js" | "jsx" => "javascript".to_string(),
            "json" => "json".to_string(),
            "md" => "markdown".to_string(),
            "css" | "scss" => "css".to_string(),
            "toml" => "toml".to_string(),
            "yaml" | "yml" => "yaml".to_string(),
            "html" => "html".to_string(),
            "py" => "python".to_string(),
            "go" => "go".to_string(),
            "c" | "h" => "c".to_string(),
            "cpp" | "hpp" | "cc" => "cpp".to_string(),
            _ => ext.to_string(),
        }
    } else {
        "plaintext".to_string()
    }
}

#[component]
pub fn FileTree() -> Element {
    let workspace = use_workspace_store();
    let editor = use_editor_store();

    let active_dir = workspace.read().active_space_id.as_ref().and_then(|id| {
        workspace
            .read()
            .spaces
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.dir.clone())
    });

    let nodes = use_signal(Vec::new);
    let loading = use_signal(|| true);

    // Fetch directory contents when active_dir changes.
    {
        let dir_for_effect = active_dir.clone();
        let mut nodes_for_effect = nodes;
        let mut loading_for_effect = loading;
        use_effect(move || {
            if let Some(dir_path) = dir_for_effect.clone() {
                loading_for_effect.set(true);
                spawn(async move {
                    match tauri_bridge::fs_list_dir(&dir_path).await {
                        Ok(response) => {
                            nodes_for_effect.set(parse_dir_entries(&response));
                            loading_for_effect.set(false);
                        }
                        Err(_) => {
                            nodes_for_effect.set(Vec::new());
                            loading_for_effect.set(false);
                        }
                    }
                });
            } else {
                nodes_for_effect.set(Vec::new());
                loading_for_effect.set(false);
            }
        });
    }

    // Subscribe to fs:change:* events to auto-refresh the tree.
    {
        let unlisteners: Rc<RefCell<Vec<Box<dyn FnOnce()>>>> =
            use_hook(|| Rc::new(RefCell::new(Vec::new())));
        let unlisteners_clone = unlisteners.clone();
        let dir_for_effect = active_dir.clone();

        use_effect(move || {
            let dir_path = dir_for_effect.clone();
            let mut nodes_for_listen = nodes;
            let mut loading_for_listen = loading;

            if let Ok(u) = tauri_bridge::listen("fs:change:*", move |_payload: String| {
                if let Some(ref dir) = dir_path {
                    loading_for_listen.set(true);
                    let dir = dir.clone();
                    spawn(async move {
                        match tauri_bridge::fs_list_dir(&dir).await {
                            Ok(response) => {
                                nodes_for_listen.set(parse_dir_entries(&response));
                                loading_for_listen.set(false);
                            }
                            Err(_) => {
                                nodes_for_listen.set(Vec::new());
                                loading_for_listen.set(false);
                            }
                        }
                    });
                }
            }) {
                unlisteners_clone.borrow_mut().push(u);
            }
        });

        let unlisteners_drop = unlisteners.clone();
        use_drop(move || {
            for unlisten in unlisteners_drop.borrow_mut().drain(..) {
                unlisten();
            }
        });
    }

    // Handler: open a file in the editor.
    let on_file_open = move |file_path: String| {
        let mut editor_for_open = editor;
        spawn(async move {
            match tauri_bridge::fs_read_file(&file_path).await {
                Ok(content) => {
                    let language = detect_language(&file_path);
                    let file = EditorFile {
                        path: file_path.clone(),
                        content,
                        language,
                        is_dirty: false,
                        cursor_position: Default::default(),
                    };
                    editor_for_open.write().open_file(file);
                }
                Err(e) => {
                    log::warn!("Failed to read file {}: {:?}", file_path, e);
                }
            }
        });
    };

    let active_dir_display = workspace.read().active_space_id.as_ref().and_then(|id| {
        workspace
            .read()
            .spaces
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.dir.clone())
    });

    rsx! {
        div {
            class: "file-tree",
            style: "padding: 4px 0;",

            if let Some(dir) = active_dir_display {
                if loading() {
                    div {
                        style: "padding: 8px 16px; color: var(--textDim); font-size: var(--text-sm);",
                        div {
                            style: "font-size: var(--text-2xs); margin-bottom: 4px; color: var(--textMuted); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{dir}"
                        }
                        "Loading file tree…"
                    }
                } else if nodes().is_empty() {
                    div {
                        style: "padding: 8px 16px; color: var(--textDim); font-size: var(--text-sm);",
                        div {
                            style: "font-size: var(--text-2xs); margin-bottom: 4px; color: var(--textMuted); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{dir}"
                        }
                        "Empty directory"
                    }
                } else {
                    for node in nodes().iter() {
                        FileTreeNode {
                            key: "{node.path}",
                            node: node.clone(),
                            depth: 0,
                            on_file_open: on_file_open.clone(),
                        }
                    }
                }
            } else {
                EmptyState {
                    kind: EmptyArt::Files,
                    title: "No workspace".to_string(),
                    hint: Some("Open a workspace to browse files.".to_string()),
                }
            }
        }
    }
}
