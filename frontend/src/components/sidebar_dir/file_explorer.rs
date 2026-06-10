use super::file_tree::FileTree;
use dioxus::prelude::*;

#[component]
pub fn FileExplorer() -> Element {
    let is_loading = use_signal(|| false);

    rsx! {
        div {
            class: "file-explorer",
            style: "display: flex; flex-direction: column; height: 100%;",

            if is_loading() {
                div {
                    style: "flex: 1; overflow-y: auto; padding: 16px; text-align: center; color: var(--textDim); font-size: var(--text-sm);",
                    "Loading..."
                }
            } else {
                div {
                    style: "flex: 1; overflow-y: auto;",
                    FileTree {}
                }
            }
        }
    }
}
