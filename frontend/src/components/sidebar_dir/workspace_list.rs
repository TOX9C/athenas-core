use crate::components::shared::icon::IconClose;
use crate::components::shared::illustration::{EmptyArt, EmptyState};
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
                EmptyState {
                    kind: EmptyArt::Workspace,
                    title: "No workspaces".to_string(),
                    hint: Some("Create one to get started.".to_string()),
                }
            } else {
                for space in spaces.iter() {
                    {
                        let is_active = active_space_id.as_deref() == Some(&space.id);
                        let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                        let text_color = if is_active { "var(--text)" } else { "var(--textMuted)" };
                        let font_weight = if is_active { "600" } else { "400" };
                        let space_id = space.id.clone();
                        let space_id_close = space.id.clone();
                        let space_name = space.name.clone();
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
                                class: "workspace-row",
                                style: "display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-radius: var(--radius-sm); cursor: pointer; background: {bg}; transition: background var(--dur-fast) var(--ease);",
                                onclick: move |_| {
                                    workspace_state.write().set_active_space(&space_id);
                                },

                                if is_active {
                                    div {
                                        style: "width: 6px; height: 6px; border-radius: 50%; background: var(--accent); flex-shrink: 0;",
                                    }
                                } else {
                                    div {
                                        style: "width: 6px; height: 6px; flex-shrink: 0;",
                                    }
                                }

                                div {
                                    style: "font-size: var(--text-xs); color: {text_color}; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: {font_weight};",
                                    "{space_name}"
                                }

                                // Agent count badges
                                if idle_count > 0 {
                                    span {
                                        class: "badge",
                                        style: "color: var(--textDim);",
                                        "{idle_count}"
                                    }
                                }

                                if running_count > 0 {
                                    span {
                                        class: "badge",
                                        style: "color: var(--warning); border-color: var(--warning);",
                                        "{running_count}"
                                    }
                                }

                                button {
                                    class: "icon-btn",
                                    style: "width: 22px; height: 22px;",
                                    title: "Close workspace",
                                    "aria-label": "Close workspace",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        workspace_state.write().remove_space(&space_id_close);
                                    },
                                    IconClose { size: Some(13), color: Some("currentColor".to_string()) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
