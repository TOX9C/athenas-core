use crate::components::shared::context_menu::{ContextMenu, MenuItem};
use crate::components::shared::icon::IconClose;
use crate::stores::workspace::Space;
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

    rsx! {
        ContextMenu {
            items: vec![MenuItem::danger("Close workspace")],
            on_select: move |_| props.on_close.call(()),

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
                        props.on_close.call(());
                    },
                    IconClose { size: Some(12), color: Some("currentColor".to_string()) }
                }
            }
        }
    }
}
