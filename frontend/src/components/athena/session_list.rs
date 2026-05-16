use crate::stores::athena::use_athena_store;
use dioxus::prelude::*;

#[component]
pub fn SessionList() -> Element {
    // TODO: wire to athena session store when available
    let athena_state = use_athena_store();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100%;",

            // Header
            div {
                style: "padding: 8px 10px; border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between;",
                span {
                    style: "font-size: 11px; font-weight: 600; color: var(--text);",
                    "Sessions"
                }
                button {
                    style: "padding: 2px 6px; border-radius: 4px; border: none; background: var(--accent); color: #fff; cursor: pointer; font-size: 10px;",
                    onclick: move |_| {
                        // TODO: create new session via Tauri IPC
                    },
                    "+"
                }
            }

            // Session items
            div {
                style: "flex: 1; overflow-y: auto;",

                if athena_state.read().messages.is_empty() {
                    div {
                        style: "padding: 16px; text-align: center; color: var(--textDim); font-size: 10px;",
                        "No sessions yet"
                    }
                } else {
                    div {
                        style: "padding: 8px 10px; border-bottom: 1px solid var(--border); cursor: pointer; background: var(--bgTertiary);",
                        div {
                            style: "font-size: 11px; font-weight: 500; color: var(--text);",
                            "Current Session"
                        }
                        div {
                            style: "font-size: 9px; color: var(--textDim); margin-top: 2px;",
                            "now"
                        }
                    }
                }
            }
        }
    }
}
