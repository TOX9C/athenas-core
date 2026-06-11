use super::workspace_tab::WorkspaceTab;
use crate::components::shared::icon::IconPlus;
use crate::stores::workspace::{use_workspace_store, Space};
use dioxus::prelude::*;
use std::rc::Rc;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabsProps {
    pub on_new_space: EventHandler<()>,
}

#[component]
pub fn WorkspaceTabs(props: WorkspaceTabsProps) -> Element {
    let workspace_state = use_workspace_store();

    // Wrap each space in `Rc<Space>` so the child component receives a cheap
    // refcount handle instead of a full clone of `Space` (which contains a
    // `Vec<PaneConfig>` and a `String` `name`). We iterate the store directly
    // to avoid cloning the outer `Vec<Space>`.
    let space_handles: Vec<Rc<Space>> = workspace_state
        .read()
        .spaces
        .iter()
        .map(Rc::new)
        .collect();

    let mut items: Vec<Element> = Vec::with_capacity(space_handles.len());
    for space_rc in space_handles {
        let space_id_on_select = space_rc.id.clone();
        let space_id_on_close = space_rc.id.clone();
        let active_space_id = workspace_state.read().active_space_id.clone();
        let is_active = active_space_id.as_deref() == Some(space_rc.id.as_str());
        let space_id_key = space_rc.id.clone();
        items.push(rsx! {
            WorkspaceTab {
                key: "{space_id_key}",
                space: space_rc,
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
                class: "icon-btn",
                style: "margin-left: 4px;",
                title: "New Workspace (Cmd+T)",
                "aria-label": "New Workspace",
                onclick: move |_| props.on_new_space.call(()),
                IconPlus { size: Some(15), color: Some("currentColor".to_string()) }
            }
        }
    }
}
