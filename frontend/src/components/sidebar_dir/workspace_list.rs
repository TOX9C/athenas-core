use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use crate::stores::workspace::use_workspace_store;
use dioxus::prelude::*;

#[component]
pub fn WorkspaceList() -> Element {
    let mut workspace_state = use_workspace_store();
    let agent_status = use_agent_status_store();
    let spaces = workspace_state.read().spaces.clone();
    let active_space_id = workspace_state.read().active_space_id.clone();

    rsx! {
        div {
            class: "workspace-list",
            style: "display: flex; flex-direction: column; gap: 2px; padding: 4px 0;",

            if spaces.is_empty() {
                div {
                    style: "padding: 16px; text-align: center; color: var(--textDim); font-size: 10px;",
                    "No workspaces yet"
                }
            } else {
                for space in spaces.iter() {
                    {
                        let is_active = active_space_id.as_deref() == Some(&space.id);
                        let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                        let space_id = space.id.clone();
                        let space_id_close = space.id.clone();
                        let space_color = space.color.clone();
                        let space_name = space.name.clone();
                        let grid_label = format!("{:?}", space.grid);

                        // Count agent statuses for this space's panes
                        let mut idle_count = 0usize;
                        let mut running_count = 0usize;
                        for pane in space.panes.iter() {
                            if let Some(status) = agent_status.read().statuses.iter()
                                .find(|(id, _)| id == &pane.id)
                                .map(|(_, s)| &s.status)
                            {
                                match status {
                                    AgentRunStatus::Working | AgentRunStatus::Thinking => running_count += 1,
                                    AgentRunStatus::Idle | AgentRunStatus::Completed => idle_count += 1,
                                    _ => {}
                                }
                            } else {
                                idle_count += 1;
                            }
                        }

                        rsx! {
                            div {
                                key: "{space_id}",
                                style: "display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-radius: 4px; cursor: pointer; background: {bg}; transition: background 0.1s;",
                                onclick: move |_| {
                                    workspace_state.write().set_active_space(&space_id);
                                },

                                div {
                                    style: "width: 8px; height: 8px; border-radius: 50%; background: {space_color}; flex-shrink: 0;",
                                }

                                span {
                                    style: "font-size: 11px; color: var(--text); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                    "{space_name}"
                                }

                                // Agent count badges
                                if idle_count > 0 {
                                    span {
                                        style: "display: inline-flex; align-items: center; justify-content: center; min-width: 18px; height: 18px; border-radius: 9px; font-size: 10px; font-weight: 600; background: rgba(255,255,255,0.15); color: var(--text);",
                                        "{idle_count}"
                                    }
                                }

                                if running_count > 0 {
                                    span {
                                        style: "display: inline-flex; align-items: center; justify-content: center; min-width: 18px; height: 18px; border-radius: 9px; font-size: 10px; font-weight: 600; background: rgba(224,108,117,0.25); color: #e06c75;",
                                        "{running_count}"
                                    }
                                }

                                span {
                                    style: "font-size: 8px; padding: 1px 3px; border-radius: 3px; background: var(--bgTertiary); color: var(--textDim);",
                                    "{grid_label}"
                                }

                                button {
                                    style: "padding: 2px; border-radius: 3px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 10px; opacity: 0.4;",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        workspace_state.write().remove_space(&space_id_close);
                                    },
                                    "\u{00d7}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
