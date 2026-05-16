use super::editor_tabs::EditorTabs;
use crate::stores::editor::use_editor_store;
use crate::utils::highlighter::highlight_code;
use dioxus::prelude::*;

#[component]
pub fn EditorPanel() -> Element {
    let mut editor_state = use_editor_store();

    let active_file = editor_state
        .read()
        .active_file_path
        .as_ref()
        .and_then(|path| {
            editor_state
                .read()
                .open_files
                .iter()
                .find(|f| f.path == *path)
                .cloned()
        });

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%; border-left: 1px solid var(--border); background: var(--bg); overflow: hidden;",

            EditorTabs {
                on_close: move |path: String| {
                    editor_state.write().close_file(&path);
                }
            }

            if let Some(file) = active_file {
                div {
                    style: "flex: 1; min-height: 0; overflow: auto; padding: 8px 0; font-family: monospace; font-size: 12px; line-height: 1.6; color: var(--text); background: var(--bg);",
                    dangerous_inner_html: "{highlight_code(&file.content, &file.language)}",
                }
            } else {
                div {
                    style: "flex: 1; display: flex; align-items: center; justify-content: center;",

                    div {
                        style: "display: flex; flex-direction: column; align-items: center; gap: 8px; color: var(--textDim);",

                        span {
                            style: "font-size: 24px; font-weight: 700; opacity: 0.25; color: var(--accent);",
                            "E"
                        }

                        span {
                            style: "font-size: 12px;",
                            "Open a file to edit"
                        }
                    }
                }
            }
        }
    }
}
