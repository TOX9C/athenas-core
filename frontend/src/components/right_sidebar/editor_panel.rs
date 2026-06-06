use crate::stores::editor::use_editor_store;
use dioxus::prelude::*;

/// Editor panel for the right sidebar.
/// Shows open files as tabs with editable content.
#[component]
pub fn RightEditorPanel() -> Element {
    let mut editor_state = use_editor_store();

    // Pre-compute tab entries to avoid inline conditionals in style strings.
    let tabs: Vec<TabEntry> = editor_state
        .read()
        .open_files
        .iter()
        .map(|file| {
            let is_active = editor_state.read().active_file_path.as_deref() == Some(&file.path);
            let bg = if is_active { "var(--bg)" } else { "transparent" };
            let color = if is_active { "var(--text)" } else { "var(--textDim)" };
            let border = if is_active { "1px solid var(--border)" } else { "1px solid transparent" };
            TabEntry {
                path: file.path.clone(),
                filename: file.path.split('/').last().unwrap_or(&file.path).to_string(),
                style: format!(
                    "display:flex;align-items:center;gap:6px;padding:6px 12px;border-bottom:{border};background:{bg};color:{color};font-size:11px;cursor:pointer;white-space:nowrap;transition:background 0.15s ease;",
                ),
            }
        })
        .collect();

    let active = editor_state.read().active_file_path.clone();

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; background: var(--bg); color: var(--text); overflow: hidden;",

            if editor_state.read().open_files.is_empty() {
                div {
                    style: "flex: 1; display: flex; align-items: center; justify-content: center; color: var(--textDim); font-size: 12px;",
                    "No files open"
                }
            } else {
                // Tab bar
                div {
                    style: "display: flex; align-items: center; gap: 0; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0; overflow-x: auto;",

                    for entry in tabs {
                        div {
                            key: "{entry.path}",
                            style: "{entry.style}",
                            onclick: {
                                let p = entry.path.clone();
                                move |_| {
                                    editor_state.write().set_active_file(p.clone());
                                }
                            },
                            "{entry.filename}"
                            button {
                                style: "padding: 0 2px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 10px; line-height: 1; transition: color 0.15s ease;",
                                onclick: {
                                    let p = entry.path.clone();
                                    move |e| {
                                        e.stop_propagation();
                                        editor_state.write().close_file(&p);
                                    }
                                },
                                "×"
                            }
                        }
                    }
                }

                // Content area
                div {
                    style: "flex: 1; display: flex; flex-direction: column; min-height: 0; overflow: hidden;",

                    if let Some(file) = editor_state.read().open_files.iter().find(|f| Some(&f.path) == active.as_ref()) {
                        div {
                            style: "flex: 1; display: flex; flex-direction: column; padding: 12px 12px 0 12px;",

                            div {
                                style: "display: flex; align-items: center; gap: 8px; padding-bottom: 8px; border-bottom: 1px solid var(--border); margin-bottom: 8px;",
                                span {
                                    style: "font-size: 11px; font-weight: 600; color: var(--text); flex: 1;",
                                    "{file.path}"
                                }
                                span {
                                    style: "font-size: 9px; padding: 2px 6px; border-radius: 3px; background: var(--bgTertiary); color: var(--textDim);",
                                    "{file.language}"
                                }
                            }

                            div {
                                style: "flex: 1; overflow: auto; padding: 8px; background: var(--bg); border-radius: 4px; border: 1px solid var(--border);",
                                pre {
                                    style: "margin: 0; padding: 0; font-family: var(--fontFamily, 'JetBrains Mono', monospace); font-size: 12px; line-height: 1.5; color: var(--text); white-space: pre-wrap; word-break: break-word; tab-size: 4;",
                                    "{file.content}"
                                }
                            }
                        }
                    } else {
                        div {
                            style: "flex: 1; display: flex; align-items: center; justify-content: center; color: var(--textDim); font-size: 12px;",
                            "Select a file"
                        }
                    }
                }
            }
        }
    }
}

struct TabEntry {
    path: String,
    filename: String,
    style: String,
}
