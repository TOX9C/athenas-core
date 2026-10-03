use super::file_tree::FileTree;
use dioxus::prelude::*;

#[component]
pub fn FileExplorer() -> Element {
    rsx! {
        div {
            class: "file-explorer",
            style: "display: flex; flex-direction: column; height: 100%;",

            div {
                style: "flex: 1; overflow-y: auto;",
                FileTree {}
            }
        }
    }
}
