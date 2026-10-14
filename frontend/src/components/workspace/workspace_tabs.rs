use super::workspace_tab::WorkspaceTab;
use crate::components::shared::confirm_dialog::ConfirmDialog;
use crate::components::shared::icon::IconPlus;
use crate::stores::agent_status::use_agent_status_store;
use crate::stores::workspace::{use_workspace_store, Space};
use crate::utils::space_counts::count_space_agents;
use dioxus::prelude::*;
use std::rc::Rc;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabsProps {
    pub on_new_space: EventHandler<()>,
}

#[component]
pub fn WorkspaceTabs(props: WorkspaceTabsProps) -> Element {
    let mut workspace_state = use_workspace_store();
    let agent_status = use_agent_status_store();

    // Confirm-close state lives HERE, not inside WorkspaceTab. Previously each
    // tab owned a `confirm_close: Signal<bool>` and rendered its own dialog;
    // because the parent rebuilt `Rc<Space>` handles on every render, any state
    // change remounted the tab subtree and tore the just-opened dialog down
    // after one frame (user-visible as "the topbar went a bit blurry"). Keeping
    // the pending space id at the parent makes the dialog survive tab churn.
    let mut confirm_close = use_signal(|| None::<String>);

    // Memoize per-space props so unchanged tabs keep pointer-equal `Rc<Space>`
    // and bool props across renders. Without this, `Rc::new(s.clone())` in the
    // render body produced a fresh pointer per pass, defeated `PartialEq` props,
    // and forced Dioxus to remount every tab (and any owned dialog) on every
    // workspace/agent-status bump.
    let space_handles: Vec<(Rc<Space>, bool, bool)> = use_memo(move || {
        let ws = workspace_state.read();
        let active_id = ws.active_space_id.clone();
        let snapshot = agent_status.snapshot();
        ws.spaces
            .iter()
            .map(|s| {
                let counts = count_space_agents(&s.panes, &snapshot);
                (
                    Rc::new(s.clone()),
                    active_id.as_deref() == Some(s.id.as_str()),
                    counts.working > 0 || counts.attention > 0,
                )
            })
            .collect()
    })();

    let mut items: Vec<Element> = Vec::with_capacity(space_handles.len());
    // The titlebar "+" stays hidden while there are no workspaces — the
    // welcome screen and sidebar already carry the creation affordance.
    let has_spaces = !space_handles.is_empty();
    for (space_rc, is_active, close_requires_confirm) in space_handles {
        let space_id_on_select = space_rc.id.clone();
        let space_id_on_close = space_rc.id.clone();
        let space_id_key = space_rc.id.clone();
        items.push(rsx! {
            WorkspaceTab {
                key: "{space_id_key}",
                space: space_rc,
                is_active: is_active,
                close_requires_confirm: close_requires_confirm,
                on_select: move |_| {
                    workspace_state.write().set_active_space(&space_id_on_select);
                },
                on_request_close: move |_| {
                    if close_requires_confirm {
                        confirm_close.set(Some(space_id_on_close.clone()));
                    } else {
                        workspace_state.write().remove_space(&space_id_on_close);
                    }
                },
            }
        });
    }

    // Pending confirmation metadata, resolved from the live store so the dialog
    // copy always shows the current name even if the space list re-renders.
    let pending: Option<(String, String)> = confirm_close().and_then(|pending_id| {
        let ws = workspace_state.read();
        ws.spaces
            .iter()
            .find(|s| s.id == pending_id)
            .map(|s| (s.id.clone(), s.name.clone()))
    });

    rsx! {
        div {
            class: "workspace-tabs",
            style: "display: flex; align-items: center; height: var(--tb-height); padding: 0 4px; overflow-x: auto; min-width: 0; flex: 0 1 auto;",

            {items.into_iter()}

            // Add space button
            if has_spaces {
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

        // Confirm before closing a workspace whose agents are still active —
        // closing kills their live sessions. Rendered at the parent level so a
        // per-tab remount can never unmount the dialog out from under the user.
        if let Some((pending_id, pending_name)) = pending {
            ConfirmDialog {
                title: "Close workspace".to_string(),
                message: format!(
                    "Close \"{pending_name}\"? Its agents are still active and will be stopped."
                ),
                confirm_label: "Close Workspace".to_string(),
                on_cancel: move |_| confirm_close.set(None),
                on_confirm: move |_| {
                    confirm_close.set(None);
                    workspace_state.write().remove_space(&pending_id);
                },
            }
        }
    }
}
