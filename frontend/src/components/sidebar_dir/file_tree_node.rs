use super::file_tree::FileNode;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct FileTreeNodeProps {
    pub node: FileNode,
    #[props(default = 0)]
    pub depth: u32,
    pub on_file_open: Callback<String>,
}

#[component]
pub fn FileTreeNode(props: FileTreeNodeProps) -> Element {
    let mut expanded = use_signal(|| props.node.is_expanded);
    let indent = (props.depth * 16) as i32;
    let chevron_rotation: i32 = if expanded() { 90 } else { 0 };

    let (icon_text, icon_color) = if props.node.is_dir {
        if expanded() {
            (">", "var(--accent)")
        } else {
            ("+", "var(--textDim)")
        }
    } else {
        match props.node.name.rsplit('.').next() {
            Some("rs") => ("rs", "#dea584"),
            Some("ts") | Some("tsx") => ("ts", "#3178c6"),
            Some("js") | Some("jsx") => ("js", "#f7df1e"),
            Some("json") => ("json", "#cbcb41"),
            Some("md") => ("md", "#519aba"),
            Some("css") | Some("scss") => ("css", "#563d7c"),
            Some("toml") => ("cfg", "#9b499c"),
            Some("yaml") | Some("yml") => ("cfg", "#9b499c"),
            _ => ("doc", "var(--textDim)"),
        }
    };

    let node_for_click = props.node.clone();
    let on_file_open = props.on_file_open.clone();

    rsx! {
        div {
            class: "file-tree-node",

            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 2px 8px 2px {indent}px; cursor: pointer; border-radius: 4px; transition: background 0.1s; font-size: 11px; color: var(--textMuted);",

                onmouseover: move |_e| {},

                onclick: move |_| {
                    if node_for_click.is_dir {
                        expanded.set(!expanded());
                    } else {
                        on_file_open.call(node_for_click.path.clone());
                    }
                },

                if props.node.is_dir {
                    span {
                        style: "font-size: 8px; width: 10px; text-align: center; transition: transform 0.15s; transform: rotate({chevron_rotation}deg);",
                        "\u{25b6}"
                    }
                } else {
                    span { style: "width: 10px;" }
                }

                if props.node.is_dir {
                    span {
                        style: "font-size: 11px; font-weight: 700; color: {icon_color}; min-width: 12px; text-align: center;",
                        "{icon_text}"
                    }
                } else {
                    span {
                        style: "font-size: 8px; font-weight: 700; color: {icon_color}; background: {icon_color}18; padding: 1px 3px; border-radius: 3px; line-height: 1;",
                        "{icon_text}"
                    }
                }

                span {
                    style: "overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text);",
                    "{props.node.name}"
                }
            }

            if props.node.is_dir && expanded() {
                for child in props.node.children.iter() {
                    FileTreeNode {
                        key: "{child.path}",
                        node: child.clone(),
                        depth: props.depth + 1,
                        on_file_open: props.on_file_open.clone(),
                    }
                }
            }
        }
    }
}
