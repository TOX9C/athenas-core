use super::workspace_tab::WorkspaceTab;
use crate::stores::workspace::{use_workspace_store, Space};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabsProps {
    pub on_new_space: EventHandler<()>,
}

#[component]
pub fn WorkspaceTabs(props: WorkspaceTabsProps) -> Element {
    let mut workspace_state = use_workspace_store();
    let spaces: Vec<Space> = workspace_state.read().spaces.clone();
    let active_space_id = workspace_state.read().active_space_id.clone();

    let mut items: Vec<Element> = Vec::new();
    for space in spaces {
        let space_id_on_select = space.id.clone();
        let space_id_on_close = space.id.clone();
        let is_active = active_space_id.as_deref() == Some(&space.id);
        let space_id_key = space.id.clone();
        items.push(rsx! {
            WorkspaceTab {
                key: "{space_id_key}",
                space: space,
                is_active: is_active,
                on_select: move |_| {
                    workspace_state.write().set_active_space(&space_id_on_select);
                },
                on_close: move |_| {
                    workspace_state.write().remove_space(&space_id_on_close);
                },
            }
        });
    }

    rsx! {
        div {
            class: "workspace-tabs",
            style: "display: flex; align-items: center; height: 32px; padding: 0 4px; overflow-x: auto; flex-shrink: 0;",

            {items.into_iter()}

            // Add space button
            button {
                style: "padding: 2px 8px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 14px; margin-left: 4px; transition: color 0.1s;",
                title: "New Workspace (Cmd+T)",
                onclick: move |_| props.on_new_space.call(()),
                "+"
            }
        }
    }
}
