use super::file_tree::FileTree;
use dioxus::prelude::*;

#[component]
pub fn FileExplorer() -> Element {
    let is_loading = use_signal(|| false);

    rsx! {
        div {
            class: "file-explorer",
            style: "display: flex; flex-direction: column; height: 100%;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; border-bottom: 1px solid var(--border);",

                span {
                    style: "font-size: 10px; font-weight: 600; color: var(--text);",
                    "Files"
                }

                button {
                    style: "padding: 2px 6px; border-radius: 3px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 9px; font-weight: 600; letter-spacing: 0.5px;",
                    onclick: move |_| {
                        // TODO: refresh file tree via Tauri IPC
                    },
                    "REFRESH"
                }
            }

            // File tree
            div {
                style: "flex: 1; overflow-y: auto;",

                if is_loading() {
                    div {
                        style: "padding: 16px; text-align: center; color: var(--textDim); font-size: 10px;",
                        "Loading..."
                    }
                } else {
                    FileTree {}
                }
            }
        }
    }
}
