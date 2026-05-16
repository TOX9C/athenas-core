use crate::stores::editor::{use_editor_store, EditorFile};
use crate::stores::workspace::use_workspace_store;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct QuickOpenProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn QuickOpen(props: QuickOpenProps) -> Element {
    let _workspace = use_workspace_store();
    let mut editor = use_editor_store();
    let mut query = use_signal(String::new);
    let mut selected_idx = use_signal(|| 0usize);

    // File list will be populated from Tauri IPC in a full implementation.
    // For now, start empty — the workspace store provides the active directory.
    let all_files: Vec<String> = Vec::new();

    let filtered: Vec<String> = all_files
        .iter()
        .filter(|f| {
            let q = query().to_lowercase();
            q.is_empty() || f.to_lowercase().contains(&q)
        })
        .take(20)
        .cloned()
        .collect();

    let flat_count = filtered.len();

    rsx! {
        div {
            style: "position: fixed; inset: 0; z-index: 50; display: flex; justify-content: center; padding-top: 15vh; background: rgba(0,0,0,0.4);",
            onclick: move |_e| {
                props.on_close.call(());
            },

            div {
                style: "width: 520px; max-height: 400px; display: flex; flex-direction: column; border-radius: 12px; box-shadow: 0 25px 50px rgba(0,0,0,0.4); overflow: hidden; background: var(--bgSecondary); border: 1px solid var(--border);",
                onclick: move |e| e.stop_propagation(),

                // Search input
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border);",

                    span {
                        style: "font-size: 11px; font-weight: 600; color: var(--textDim);",
                        "FIND"
                    }

                    input {
                        style: "flex: 1; background: transparent; border: none; outline: none; font-size: 13px; color: var(--text);",
                        value: "{query}",
                        oninput: move |e| {
                            query.set(e.value());
                            selected_idx.set(0);
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            match e.key() {
                                Key::ArrowDown => {
                                    selected_idx.set((selected_idx() + 1).min(flat_count.saturating_sub(1)));
                                }
                                Key::ArrowUp => {
                                    selected_idx.set(selected_idx().saturating_sub(1));
                                }
                                Key::Enter => {
                                    if let Some(path) = filtered.get(selected_idx()).cloned() {
                                        editor.write().open_file(EditorFile {
                                            path: path.clone(),
                                            ..Default::default()
                                        });
                                    }
                                    props.on_close.call(());
                                }
                                Key::Escape => {
                                    props.on_close.call(());
                                }
                                _ => {}
                            }
                        },
                        placeholder: "Search files...",
                        autocomplete: "off",
                    }
                }

                // Results
                div {
                    style: "flex: 1; overflow-y: auto;",

                    if filtered.is_empty() {
                        {
                            let empty_msg = if query().is_empty() { "Type to search" } else { "No files found" };
                            rsx! {
                                div {
                                    style: "padding: 24px 12px; text-align: center; font-size: 12px; color: var(--textDim);",
                                    "{empty_msg}"
                                }
                            }
                        }
                    } else {
                        for (idx, path) in filtered.iter().enumerate() {
                            {
                                let display_name = path.clone();
                                let item_bg = if idx == selected_idx() { "var(--bgTertiary)" } else { "transparent" };
                                let path_for_click = path.clone();
                                rsx! {
                                    button {
                                        key: "{path}",
                                        style: "display: flex; align-items: center; padding: 6px 12px; width: 100%; text-align: left; border: none; background: {item_bg}; cursor: pointer; transition: background 0.1s;",
                                        onmouseenter: move |_| selected_idx.set(idx),
                                        onclick: move |_| {
                                            editor.write().open_file(EditorFile {
                                                path: path_for_click.clone(),
                                                ..Default::default()
                                            });
                                            props.on_close.call(());
                                        },

                                        span {
                                            style: "font-size: 11px; font-family: monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text);",
                                            "{display_name}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
