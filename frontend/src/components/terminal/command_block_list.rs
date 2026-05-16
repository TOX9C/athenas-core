use super::command_block::{CommandBlock, CommandBlockData};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct CommandBlockListProps {
    pub blocks: Vec<CommandBlockData>,
}

#[component]
pub fn CommandBlockList(props: CommandBlockListProps) -> Element {
    rsx! {
        div {
            class: "command-block-list",
            style: "display: flex; flex-direction: column; gap: 4px; padding: 8px;",

            if props.blocks.is_empty() {
                div {
                    style: "text-align: center; color: var(--textDim); font-size: 11px; padding: 16px;",
                    "No command history"
                }
            } else {
                for block in props.blocks.iter() {
                    CommandBlock { key: "{block.id}", block: block.clone() }
                }
            }
        }
    }
}
