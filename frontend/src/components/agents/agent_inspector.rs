use super::agent_output_panel::AgentOutputPanel;
use super::agent_selector::AgentSelector;
use super::agent_status_bar::AgentPaneStatus;
use crate::components::shared::icon::{IconBell, IconClose, IconPulse, IconSearch, IconTerminal};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus, AgentStatus};
use crate::stores::notification::{use_notification_store, NotificationRecord, NotificationType};
use dioxus::prelude::*;

/// Inspector tab types.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InspectorTab {
    #[default]
    Output,
    Status,
    Notifications,
}

/// Convert store AgentStatus to the component-level AgentPaneStatus.
fn to_pane_status(agent_status: &AgentStatus) -> AgentPaneStatus {
    use super::agent_status_bar::ProgressInfo;
    AgentPaneStatus {
        pane_id: agent_status.pane_id.clone(),
        status: match agent_status.status {
            AgentRunStatus::Idle => "idle".to_string(),
            AgentRunStatus::Thinking => "thinking".to_string(),
            AgentRunStatus::Working => "working".to_string(),
            AgentRunStatus::WaitingForInput => "waiting_for_input".to_string(),
            AgentRunStatus::Completed => "completed".to_string(),
            AgentRunStatus::Error => "error".to_string(),
            AgentRunStatus::Cancelled => "cancelled".to_string(),
            AgentRunStatus::Disconnected => "disconnected".to_string(),
        },
        agent_type: String::new(),
        message: agent_status.message.clone().unwrap_or_default(),
        progress: agent_status.progress.as_ref().map(|p| ProgressInfo {
            current: p.current,
            total: p.total,
            label: p.label.clone().into(),
        }),
        last_updated_at: agent_status.last_updated_at,
    }
}

/// Convert a NotificationRecord to the inspector's local NotificationItem.
fn status_dot_color(status: &str) -> &'static str {
    match status {
        "idle" => "var(--textDim)",
        "thinking" => "var(--accent)",
        "working" => "var(--accent)",
        "waiting_for_input" => "var(--warning)",
        "completed" => "var(--success)",
        "error" => "var(--error)",
        "cancelled" => "var(--error)",
        "disconnected" => "var(--textDim)",
        _ => "var(--textDim)",
    }
}

fn to_notif_item(rec: &NotificationRecord) -> NotificationItem {
    NotificationItem {
        id: rec.id.clone(),
        notif_type: match rec.r#type {
            NotificationType::Info => "info",
            NotificationType::Warning => "warning",
            NotificationType::Error => "error",
            NotificationType::Success => "success",
            NotificationType::NeedsInput => "needs_input",
            NotificationType::TaskComplete => "task_complete",
            NotificationType::TaskError => "task_error",
        }
        .to_string(),
        title: rec.title.clone(),
        message: rec.message.clone(),
    }
}

#[component]
pub fn AgentInspector() -> Element {
    let mut agent_output = use_agent_output_store();
    let agent_status = use_agent_status_store();
    let notifications = use_notification_store();

    let inspector_open = agent_output.read().inspector_open;
    let selected_pane_id = agent_output.read().selected_pane_id.clone();
    let mut tab = use_signal(InspectorTab::default);
    let mut search_query = use_signal(String::new);

    let pane_status: Option<AgentPaneStatus> = selected_pane_id.as_ref().and_then(|id| {
        agent_status
            .read()
            .statuses
            .iter()
            .find(|(pid, _)| pid == id)
            .map(|(_, s)| to_pane_status(s))
    });

    let filtered_notifications: Vec<NotificationItem> = {
        let query = search_query();
        notifications
            .read()
            .iter()
            .filter(|n| {
                if let Some(pane_id) = &selected_pane_id {
                    n.source == *pane_id
                } else {
                    true
                }
            })
            .filter(|n| {
                if query.is_empty() {
                    true
                } else {
                    let q = query.to_lowercase();
                    n.title.to_lowercase().contains(&q) || n.message.to_lowercase().contains(&q)
                }
            })
            .map(to_notif_item)
            .collect()
    };

    if !inspector_open {
        return rsx! {};
    }

    // Tab definitions: (tab enum, label)
    let tabs = [
        (InspectorTab::Output, "Output"),
        (InspectorTab::Status, "Status"),
        (InspectorTab::Notifications, "Alerts"),
    ];

    let (notif_empty_title, notif_empty_hint) = if selected_pane_id.is_some() {
        ("All clear", "No notifications for this agent.")
    } else {
        ("No agent", "Select an agent to view its alerts.")
    };

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; border-left: 1px solid var(--border); width: 360px; background: var(--bgSecondary); flex-shrink: 0; position: absolute; right: 0; top: 0; bottom: 0; z-index: 90;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                AgentSelector {
                    on_select: move |id: String| {
                        agent_output.write().select_agent(Some(id));
                    }
                }

                div { style: "flex: 1;" }

                button {
                    class: "icon-btn",
                    title: "Close inspector",
                    onclick: move |_| agent_output.write().set_inspector_open(false),
                    IconClose { size: Some(15), color: Some("currentColor".to_string()) }
                }
            }

            // Tab bar — segmented
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                div {
                    class: "segmented",
                    style: "display: inline-flex;",

                    for (tab_id, label) in tabs {
                        {
                            let is_active = tab() == tab_id;
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: if is_active { "is-active" } else { "" },
                                    title: "{label}",
                                    onclick: move |_| tab.set(tab_id),
                                    span {
                                        style: "display: inline-flex; align-items: center; gap: 5px;",
                                        {match tab_id {
                                            InspectorTab::Output => rsx! { IconTerminal { size: Some(13), color: Some("currentColor".to_string()) } },
                                            InspectorTab::Status => rsx! { IconPulse { size: Some(13), color: Some("currentColor".to_string()) } },
                                            InspectorTab::Notifications => rsx! { IconBell { size: Some(13), color: Some("currentColor".to_string()) } },
                                        }}
                                        span {
                                            style: "font-family: var(--font-display); letter-spacing: 0.04em;",
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Tab content
            div {
                style: "flex: 1; min-height: 0; overflow: hidden;",

                if tab() == InspectorTab::Output {
                    AgentOutputPanel {}
                }

                if tab() == InspectorTab::Status {
                    {
                        if let Some(st) = pane_status {
                            rsx! {
                                div {
                                    style: "padding: 14px; overflow-y: auto; height: 100%; overflow-x: hidden;",

                                    div {
                                        class: "card",
                                        style: "display: flex; flex-direction: column; gap: 10px; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-md);",

                                        StatusRow { label: "Pane".to_string(), value: st.pane_id.clone() }
                                        StatusRow { label: "Status".to_string(), value: st.status.clone(), dot_color: status_dot_color(&st.status).to_string() }

                                        if !st.message.is_empty() {
                                            StatusRow { label: "Message".to_string(), value: st.message.clone() }
                                        }

                                        if let Some(progress) = &st.progress {
                                            div {
                                                div {
                                                    style: "display: flex; align-items: center; gap: 6px; font-size: var(--text-2xs); margin-bottom: 5px; color: var(--accent); font-family: var(--font-display); letter-spacing: 0.04em; text-transform: uppercase;",
                                                    "Progress"
                                                }
                                                div {
                                                    style: "display: flex; align-items: center; gap: 8px;",

                                                    div {
                                                        style: "flex: 1; height: 4px; overflow: hidden; background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-pill);",

                                                        div {
                                                            style: "height: 100%; background: var(--accent); border-radius: var(--radius-pill); width: {((progress.current * 100) / progress.total.max(1))}%;",
                                                        }
                                                    }

                                                    span {
                                                        style: "font-size: var(--text-2xs); color: var(--textDim); font-family: var(--fontFamily);",
                                                        "{progress.current}/{progress.total}"
                                                    }
                                                }

                                                if let Some(label) = &progress.label {
                                                    span {
                                                        style: "font-size: var(--text-2xs); display: block; margin-top: 3px; color: var(--textDim);",
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                EmptyState {
                                    kind: EmptyArt::Agents,
                                    title: "No agent".to_string(),
                                    hint: Some("Select an agent to inspect.".to_string()),
                                }
                            }
                        }
                    }
                }

                if tab() == InspectorTab::Notifications {
                    div {
                        style: "display: flex; flex-direction: column; height: 100%; overflow-x: hidden;",

                        // Search
                        div {
                            style: "padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                            div {
                                class: "field",
                                style: "display: flex; align-items: center; gap: 6px;",

                                IconSearch { size: Some(13), color: Some("var(--textDim)".to_string()) }

                                input {
                                    style: "flex: 1; background: transparent; border: none; outline: none; font-size: var(--text-xs); color: var(--text);",
                                    value: "{search_query}",
                                    oninput: move |e| search_query.set(e.value()),
                                    placeholder: "Filter notifications...",
                                }
                            }
                        }

                        // Notification list
                        div {
                            style: "flex: 1; overflow-y: auto; overflow-x: hidden;",

                            if filtered_notifications.is_empty() {
                                EmptyState {
                                    kind: EmptyArt::Notifications,
                                    title: notif_empty_title.to_string(),
                                    hint: Some(notif_empty_hint.to_string()),
                                }
                            } else {
                                for n in filtered_notifications.iter() {
                                    {
                                        let type_color = match n.notif_type.as_str() {
                                            "error" | "task_error" => "var(--error)",
                                            "warning" => "var(--warning)",
                                            "success" | "task_complete" => "var(--success)",
                                            "needs_input" => "var(--warning)",
                                            _ => "var(--accentTeal)",
                                        };
                                        let n_id = n.id.clone();
                                        let n_title = n.title.clone();
                                        let n_type: String = match n.notif_type.as_str() {
                                            "info" => "Info".to_string(),
                                            "warning" => "Warning".to_string(),
                                            "error" => "Error".to_string(),
                                            "success" => "Success".to_string(),
                                            "needs_input" => "Needs input".to_string(),
                                            "task_complete" => "Task done".to_string(),
                                            "task_error" => "Task error".to_string(),
                                            other => other.replace('_', " ").to_string(),
                                        };
                                        let n_msg = n.message.clone();
                                        rsx! {
                                            div {
                                                key: "{n_id}",
                                                class: "lit-sweep",
                                                style: "padding: 10px 12px; border-bottom: 1px solid var(--border); overflow-x: hidden;",

                                                div {
                                                    style: "display: flex; align-items: center; gap: 8px; overflow-x: hidden;",

                                                    span {
                                                        class: "status-label",
                                                        style: "width: 64px; flex-shrink: 0; color: {type_color};",
                                                        "{n_type}"
                                                    }

                                                    span {
                                                        style: "font-size: var(--text-xs); font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0;",
                                                        "{n_title}"
                                                    }

                                                }

                                                p {
                                                    style: "font-size: var(--text-2xs); margin-top: 3px; color: var(--textDim); overflow: hidden; text-overflow: ellipsis;",
                                                    "{n_msg}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Simple key-value status row.
#[derive(Props, Clone, PartialEq)]
struct StatusRowProps {
    label: String,
    value: String,
    #[props(default)]
    dot_color: String,
}

#[component]
fn StatusRow(props: StatusRowProps) -> Element {
    let value_color = if props.dot_color.is_empty() {
        "var(--text)"
    } else {
        props.dot_color.as_str()
    };

    rsx! {
        div {
            style: "display: flex; align-items: baseline; gap: 8px; overflow-x: hidden;",

            span {
                style: "display: inline-flex; align-items: center; gap: 5px; font-size: var(--text-2xs); flex-shrink: 0; width: 64px; color: var(--accent); font-family: var(--font-display); letter-spacing: 0.04em; text-transform: uppercase;",
                "{props.label}"
            }

            span {
                style: "font-size: var(--text-xs); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: {value_color}; flex: 1; min-width: 0;",
                "{props.value}"
            }
        }
    }
}

/// A notification item for the inspector's notification tab.
#[derive(Debug, Clone, PartialEq, Default)]
struct NotificationItem {
    id: String,
    notif_type: String,
    title: String,
    message: String,
}
