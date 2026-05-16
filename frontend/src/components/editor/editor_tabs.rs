use crate::stores::editor::{use_editor_store, EditorFile};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct EditorTabsProps {
    pub on_close: EventHandler<String>,
}

#[component]
pub fn EditorTabs(props: EditorTabsProps) -> Element {
    let mut editor_state = use_editor_store();

    let files: Vec<EditorFile> = editor_state.read().open_files.clone();

    if files.is_empty() {
        return rsx! {};
    }

    let active_path = editor_state.read().active_file_path.clone();

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 2px; overflow-x: auto; flex-shrink: 0; border-bottom: 1px solid var(--border); padding: 0 4px; height: 32px; background: var(--bgSecondary);",

            for file in files.iter() {
                {
                    let filename: String = file.path.split('/').next_back().unwrap_or(&file.path).to_string();
                    let is_active = Some(file.path.as_str()) == active_path.as_deref();
                    let path_close = file.path.clone();
                    let path_click = file.path.clone();
                    let tab_bg = if is_active { "var(--bg)" } else { "transparent" };
                    let tab_border = if is_active { "var(--accent)" } else { "transparent" };
                    let tab_color = if is_active { "var(--text)" } else { "var(--textMuted)" };
                    rsx! {
                        div {
                            key: "{file.path}",
                            style: "display: flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 6px 6px 0 0; cursor: pointer; flex-shrink: 0; background: {tab_bg}; border-bottom: 2px solid {tab_border};",
                            onclick: move |_| {
                                editor_state.write().set_active_file(path_click.clone());
                            },

                            if file.is_dirty {
                                span {
                                    style: "width: 6px; height: 6px; border-radius: 50%; background: var(--warning); flex-shrink: 0;",
                                }
                            }

                            span {
                                style: "font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 120px; color: {tab_color};",
                                "{filename}"
                            }

                            button {
                                style: "padding: 2px; border-radius: 2px; border: none; background: transparent; cursor: pointer; opacity: 0.5;",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    props.on_close.call(path_close.clone());
                                },
                                span { style: "font-size: 10px; color: var(--textDim);", "\u{2715}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
