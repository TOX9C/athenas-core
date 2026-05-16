use crate::components::plugin::agent_status_list::AgentStatusList;
use crate::components::plugin::plugin_dashboard::PluginDashboard;
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

    let (section_title, _section_label) = match section {
        SidebarSection::Spaces => ("Spaces", "SP"),
        SidebarSection::Files => ("Files", "FL"),
        SidebarSection::Agents => ("Agents", "AG"),
        SidebarSection::Plugins => ("Plugins", "PL"),
    };

    let section_accent = match section {
        SidebarSection::Spaces => "var(--accent)",
        SidebarSection::Files => "var(--warning)",
        SidebarSection::Agents => "var(--success)",
        SidebarSection::Plugins => "#a78bfa",
    };

    rsx! {
        div {
            class: "sidebar",
            style: "width: {sidebar_width}px; min-width: 180px; max-width: 400px; height: 100%; display: flex; flex-direction: column; border-right: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

            // Header
            div {
                style: "display: flex; align-items: center; justify-content: space-between; padding: 6px 10px; border-bottom: 1px solid var(--border);",

                div {
                    style: "display: flex; align-items: center; gap: 6px;",

                    // Section icon — colored dot
                    div {
                        style: "width: 6px; height: 6px; border-radius: 50%; background: {section_accent};",
                    }

                    span {
                        style: "font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--textMuted);",
                        "{section_title}"
                    }
                }

                div {
                    style: "display: flex; align-items: center; gap: 2px;",

                    if matches!(section, SidebarSection::Spaces) {
                        button {
                            style: "padding: 2px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textMuted); cursor: pointer; font-size: 14px; font-weight: 500;",
                            title: "New workspace",
                            onclick: move |_| props.on_new_space.call(()),
                            "+"
                        }
                    }

                    button {
                        style: "padding: 2px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textMuted); cursor: pointer; font-size: 10px;",
                        title: "Collapse sidebar",
                        onclick: move |_| ui_state.write().sidebar_visible = false,
                        "\u{2039}"
                    }
                }
            }

            // Content area
            div {
                style: "flex: 1; overflow-y: auto; padding: 2px 0;",

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
                    style: "border-top: 1px solid var(--border); padding: 6px 8px;",

                    button {
                        style: "width: 100%; display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-radius: 6px; border: none; background: transparent; color: var(--textMuted); cursor: pointer; font-size: 11px; font-weight: 500; transition: background 0.1s;",
                        onclick: move |_| props.on_new_space.call(()),
                        span { style: "font-size: 14px; font-weight: 500;", "+" }
                        "New Workspace"
                    }
                }
            }

            // Section tab bar
            div {
                style: "display: flex; align-items: center; justify-content: space-around; padding: 4px 4px; border-top: 1px solid var(--border); background: var(--bg); flex-shrink: 0;",

                for (sec, label) in [
                    (SidebarSection::Spaces, "SP"),
                    (SidebarSection::Files, "FL"),
                    (SidebarSection::Agents, "AG"),
                    (SidebarSection::Plugins, "PL"),
                ] {
                    {
                        let is_active = section == sec;
                        let bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                        let color = if is_active { "var(--text)" } else { "var(--textDim)" };
                        let title = match sec {
                            SidebarSection::Spaces => "Spaces",
                            SidebarSection::Files => "Files",
                            SidebarSection::Agents => "Agents",
                            SidebarSection::Plugins => "Plugins",
                        };
                        rsx! {
                            button {
                                key: "{label}",
                                style: "padding: 4px 10px; border-radius: 4px; border: none; background: {bg}; color: {color}; cursor: pointer; font-size: 9px; font-weight: 600; letter-spacing: 0.04em; transition: background 0.1s;",
                                title: "{title}",
                                onclick: move |_| ui_state.write().sidebar_section = sec,
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
