use super::file_tree::FileNode;
use crate::components::shared::icon::{IconChevronRight, IconFolder};
use crate::utils::file_icons::get_file_icon;
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
    let mut hovered = use_signal(|| false);
    let indent = (props.depth * 16) as i32;
    let chevron_rotation: i32 = if expanded() { 90 } else { 0 };

    let (icon_text, icon_color) = if props.node.is_dir {
        (String::new(), "var(--accent)")
    } else {
        let code = get_file_icon(&props.node.name);
        let ext = props
            .node
            .name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let color = match ext.as_str() {
            "rs" => "#C98A5A",
            "ts" | "tsx" => "#4C8DD6",
            "js" | "jsx" => "#C9B64B",
            "json" => "#B0B04B",
            "md" => "#5A9BC9",
            "css" | "scss" => "#6E5A9E",
            "toml" | "yaml" | "yml" => "#9B6AB0",
            "sh" | "bash" | "zsh" => "#7BAE5A",
            "py" => "#C9A24B",
            "go" => "#4F9EC9",
            _ => "var(--textDim)",
        };
        let label = if code.is_empty() {
            if props.node.name.starts_with('.') {
                "·".to_string()
            } else {
                "DOC".to_string()
            }
        } else {
            code.to_string()
        };
        (label, color)
    };

    let node_for_click = props.node.clone();
    let on_file_open = props.on_file_open;

    rsx! {
        div {
            class: "file-tree-node",

            div {
                class: "lit-sweep",
                style: "display: flex; align-items: center; gap: 4px; padding: 2px 8px 2px {indent}px; cursor: pointer; border-radius: var(--radius-sm); transition: background var(--dur-fast) var(--ease); font-size: var(--text-sm); color: var(--textMuted);",
                background: if hovered() { "var(--bgHover)" } else { "transparent" },

                onmouseenter: move |_| hovered.set(true),
                onmouseleave: move |_| hovered.set(false),

                onclick: move |_| {
                    if node_for_click.is_dir {
                        expanded.set(!expanded());
                    } else {
                        on_file_open.call(node_for_click.path.clone());
                    }
                },

                if props.node.is_dir {
                    span {
                        style: "display: inline-flex; width: 10px; transition: transform var(--dur-fast) var(--ease); transform: rotate({chevron_rotation}deg);",
                        IconChevronRight { size: Some(11), color: Some("var(--textDim)".to_string()) }
                    }
                } else {
                    span { style: "width: 10px;" }
                }

                if props.node.is_dir {
                    span {
                        style: "display: inline-flex; align-items: center;",
                        IconFolder { size: Some(14), color: Some("var(--accent)".to_string()) }
                    }
                } else {
                    span {
                        style: "font-size: 8px; font-weight: 700; letter-spacing: 0.04em; color: {icon_color}; background: color-mix(in srgb, {icon_color} 12%, transparent); padding: 2px 3.5px; border-radius: 4px; line-height: 1; font-family: var(--fontFamily);",
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
                        on_file_open: props.on_file_open,
                    }
                }
            }
        }
    }
}
