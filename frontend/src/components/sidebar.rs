use crate::components::plugin::agent_status_list::AgentStatusList;
use crate::components::plugin::plugin_dashboard::PluginDashboard;
use crate::components::shared::icon::{
    IconAgents, IconChevronLeft, IconFiles, IconGrid, IconPlugins, IconPlus,
};
use crate::components::sidebar_dir::file_explorer::FileExplorer;
use crate::components::sidebar_dir::workspace_list::WorkspaceList;
use crate::stores::ui::{use_ui_store, SidebarSection};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SidebarProps {
    pub on_new_space: EventHandler<()>,
}

#[component]
pub fn Sidebar(props: SidebarProps) -> Element {
    let mut ui_state = use_ui_store();
    let section = ui_state.read().sidebar_section;
    let sidebar_width = ui_state.read().sidebar_width;

    let section_title = match section {
        SidebarSection::Spaces => "Spaces",
        SidebarSection::Files => "Files",
        SidebarSection::Agents => "Agents",
        SidebarSection::Plugins => "Plugins",
    };

    rsx! {
        div {
            class: "sidebar",
            role: "navigation",
            "aria-label": "Sidebar",
            style: "width: {sidebar_width}px; min-width: 180px; max-width: 400px; height: 100%; display: flex; flex-direction: column; border-right: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 6px 10px; border-bottom: 1px solid var(--border);",

                span {
                    style: "display: flex; align-items: center; gap: 7px; font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; letter-spacing: 0.01em; color: var(--text); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",

                    {match section {
                        SidebarSection::Spaces => rsx! { IconGrid { size: Some(15), color: Some("var(--accent)".to_string()) } },
                        SidebarSection::Files => rsx! { IconFiles { size: Some(15), color: Some("var(--accent)".to_string()) } },
                        SidebarSection::Agents => rsx! { IconAgents { size: Some(15), color: Some("var(--accent)".to_string()) } },
                        SidebarSection::Plugins => rsx! { IconPlugins { size: Some(15), color: Some("var(--accent)".to_string()) } },
                    }}

                    "{section_title}"
                }

                div {
                    style: "display: flex; align-items: center; gap: 4px;",

                    button {
                        class: "icon-btn",
                        title: "Collapse sidebar",
                        "aria-label": "Collapse sidebar",
                        onclick: move |_| ui_state.write().sidebar_visible = false,
                        IconChevronLeft { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                }
            }

            // Content area
            div {
                style: "flex: 1; overflow-y: auto; padding: 8px 0;",

                match section {
                    SidebarSection::Spaces => rsx! {
                        WorkspaceList {}
                    },
                    SidebarSection::Files => rsx! {
                        FileExplorer {}
                    },
                    SidebarSection::Agents => rsx! {
                        AgentStatusList {}
                    },
                    SidebarSection::Plugins => rsx! {
                        PluginDashboard {}
                    },
                }
            }

            // Bottom action bar (spaces section only)
            if matches!(section, SidebarSection::Spaces) {
                div {
                    style: "border-top: 1px solid var(--border); padding: 10px 12px;",

                    button {
                        class: "btn-secondary btn-sm",
                        style: "width: 100%; display: flex; align-items: center; justify-content: center; gap: 6px;",
                        onclick: move |_| {
                            web_sys::console::log_1(&"[Sidebar] Bottom New Workspace clicked".into());
                            props.on_new_space.call(());
                        },
                        IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                        "New Workspace"
                    }
                }
            }

            // Section tab bar
            div {
                style: "display: flex; align-items: center; justify-content: space-around; padding: 8px 8px; border-top: 1px solid var(--border); background: var(--bg); flex-shrink: 0;",

                for (sec, label) in [
                    (SidebarSection::Spaces, "SP"),
                    (SidebarSection::Files, "FL"),
                    (SidebarSection::Agents, "AG"),
                    (SidebarSection::Plugins, "PL"),
                ] {
                    {
                        let is_active = section == sec;
                        let color = if is_active { "var(--accent)" } else { "var(--textDim)" };
                        let title = match sec {
                            SidebarSection::Spaces => "Spaces",
                            SidebarSection::Files => "Files",
                            SidebarSection::Agents => "Agents",
                            SidebarSection::Plugins => "Plugins",
                        };
                        rsx! {
                            button {
                                key: "{label}",
                                class: if is_active { "icon-btn is-active" } else { "icon-btn" },
                                title: "{title}",
                                "aria-label": "{title}",
                                onclick: move |_| ui_state.write().sidebar_section = sec,
                                {match sec {
                                    SidebarSection::Spaces => rsx! { IconGrid { size: Some(15), color: Some(color.to_string()) } },
                                    SidebarSection::Files => rsx! { IconFiles { size: Some(15), color: Some(color.to_string()) } },
                                    SidebarSection::Agents => rsx! { IconAgents { size: Some(15), color: Some(color.to_string()) } },
                                    SidebarSection::Plugins => rsx! { IconPlugins { size: Some(15), color: Some(color.to_string()) } },
                                }}
                            }
                        }
                    }
                }
            }
        }
    }
}
