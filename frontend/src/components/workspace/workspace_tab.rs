use crate::components::shared::confirm_dialog::ConfirmDialog;
use crate::components::shared::context_menu::{ContextMenu, MenuItem};
use crate::components::shared::icon::IconClose;
use crate::stores::agent_status::use_agent_status_store;
use crate::stores::workspace::Space;
use crate::utils::space_counts::count_space_agents;
use dioxus::prelude::*;
use std::rc::Rc;

#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceTabProps {
    pub space: Rc<Space>,
    pub is_active: bool,
    pub on_select: EventHandler<()>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn WorkspaceTab(props: WorkspaceTabProps) -> Element {
    // Selected space is marked by gold + bold text only — no bottom-edge
    // hairline underline. Matches the app vocabulary (.icon-btn.is-active,
    // .segmented-item.is-active both signal active via color, not a rule).
    let text_color = if props.is_active {
        "var(--accent)"
    } else {
        "var(--textMuted)"
    };
    let weight = if props.is_active { "600" } else { "400" };

    // Closing a workspace that still has active agents kills their sessions,
    // so that path is gated behind an explicit confirm (idle/shell-only
    // workspaces close immediately). Mirrors the sidebar row behavior.
    let agent_status = use_agent_status_store();
    let mut confirm_close = use_signal(|| false);
    let counts = count_space_agents(&props.space.panes, &agent_status.read().statuses);
    let close_requires_confirm = counts.working > 0 || counts.attention > 0;
    let space_name_for_confirm = props.space.name.clone();

    rsx! {
        ContextMenu {
            items: vec![MenuItem::danger("Close workspace")],
            on_select: move |_| {
                if close_requires_confirm {
                    confirm_close.set(true);
                } else {
                    props.on_close.call(());
                }
            },

            div {
                class: "workspace-tab",
                style: "display: flex; align-items: center; gap: 6px; height: var(--tb-tab-height); padding: 0 10px; border: none; border-radius: var(--radius-sm); cursor: pointer; background: transparent; flex-shrink: 0; transition: color var(--dur-fast) var(--ease), background-color var(--dur-fast) var(--ease);",
                onclick: move |_| props.on_select.call(()),

                span {
                    style: "font-size: var(--tb-tab-font); font-weight: {weight}; color: {text_color}; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; letter-spacing: 0.02em;",
                    "{props.space.name}"
                }

                // Close button
                button {
                    class: "icon-btn",
                    style: "width: 20px; height: 20px;",
                    title: "Close workspace",
                    "aria-label": "Close workspace",
                    onclick: move |e| {
                        e.stop_propagation();
                        if close_requires_confirm {
                            confirm_close.set(true);
                        } else {
                            props.on_close.call(());
                        }
                    },
                    IconClose { size: Some(12), color: Some("currentColor".to_string()) }
                }
            }

            // Confirm before closing a workspace whose agents are still
            // active — closing kills their live sessions.
            if confirm_close() {
                ConfirmDialog {
                    title: "Close workspace".to_string(),
                    message: format!(
                        "Close \"{space_name_for_confirm}\"? Its agents are still active and will be stopped."
                    ),
                    confirm_label: "Close Workspace".to_string(),
                    on_cancel: move |_| confirm_close.set(false),
                    on_confirm: move |_| {
                        confirm_close.set(false);
                        props.on_close.call(());
                    },
                }
            }
        }
    }
}
