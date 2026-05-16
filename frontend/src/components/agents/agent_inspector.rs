use super::agent_output_panel::AgentOutputPanel;
use super::agent_selector::AgentSelector;
use super::agent_status_bar::AgentPaneStatus;
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

    // Tab definitions: (tab enum, short text icon, label)
    let tabs = [
        (InspectorTab::Output, "OUT", "Output"),
        (InspectorTab::Status, "STS", "Status"),
        (InspectorTab::Notifications, "ALT", "Alerts"),
    ];

    let notif_empty_msg = if selected_pane_id.is_some() {
        "No notifications for this agent"
    } else {
        "Select an agent"
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; border-left: 1px solid var(--border); width: 360px; background: var(--bgSecondary); flex-shrink: 0;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                AgentSelector {
                    on_select: move |id: String| {
                        agent_output.write().select_agent(Some(id));
                    }
                }

                div { style: "flex: 1;" }

                button {
                    style: "padding: 4px; border-radius: 4px; border: none; background: transparent; cursor: pointer;",
                    title: "Close inspector",
                    onclick: move |_| agent_output.write().set_inspector_open(false),
                    span { style: "font-size: 12px; color: var(--textDim);", "\u{2715}" }
                }
            }

            // Tab bar
            div {
                style: "display: flex; align-items: center; gap: 2px; padding: 4px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                for (tab_id, icon, label) in tabs {
                    {
                        let is_active = tab() == tab_id;
                        let tab_bg = if is_active { "var(--bgTertiary)" } else { "transparent" };
                        let tab_color = if is_active { "var(--text)" } else { "var(--textDim)" };
                        rsx! {
                            button {
                                key: "{label}",
                                style: "display: flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: 4px; border: none; font-size: 10px; font-weight: 500; cursor: pointer; background: {tab_bg}; color: {tab_color};",
                                onclick: move |_| tab.set(tab_id),
                                span {
                                    style: "font-size: 8px; font-weight: 700; letter-spacing: 0.04em; opacity: 0.7;",
                                    "{icon}"
                                }
                                "{label}"
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
                                    style: "padding: 12px; overflow-y: auto; height: 100%;",

                                    StatusRow { label: "Pane".to_string(), value: st.pane_id.clone() }
                                    StatusRow { label: "Status".to_string(), value: st.status.clone() }

                                    if !st.message.is_empty() {
                                        StatusRow { label: "Message".to_string(), value: st.message.clone() }
                                    }

                                    if let Some(progress) = &st.progress {
                                        div {
                                            div {
                                                style: "font-size: 9px; margin-bottom: 4px; color: var(--textDim);",
                                                "Progress"
                                            }
                                            div {
                                                style: "display: flex; align-items: center; gap: 8px;",

                                                div {
                                                    style: "flex: 1; height: 4px; border-radius: 9999px; overflow: hidden; background: var(--bgTertiary);",

                                                    div {
                                                        style: "height: 100%; border-radius: 9999px; background: var(--accent); width: {((progress.current * 100) / progress.total.max(1))}%;",
                                                    }
                                                }

                                                span {
                                                    style: "font-size: 9px; color: var(--textDim);",
                                                    "{progress.current}/{progress.total}"
                                                }
                                            }

                                            if let Some(label) = &progress.label {
                                                span {
                                                    style: "font-size: 8px; display: block; margin-top: 2px; color: var(--textDim);",
                                                    "{label}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                div {
                                    style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--textDim);",
                                    span { style: "font-size: 10px;", "Select an agent to view status" }
                                }
                            }
                        }
                    }
                }

                if tab() == InspectorTab::Notifications {
                    div {
                        style: "display: flex; flex-direction: column; height: 100%;",

                        // Search
                        div {
                            style: "padding: 4px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0;",

                            div {
                                style: "display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-radius: 4px; background: var(--bgTertiary);",

                                span {
                                    style: "font-size: 8px; font-weight: 700; color: var(--textDim); letter-spacing: 0.04em;",
                                    "SRCH"
                                }

                                input {
                                    style: "flex: 1; background: transparent; border: none; outline: none; font-size: 10px; color: var(--text);",
                                    value: "{search_query}",
                                    oninput: move |e| search_query.set(e.value()),
                                    placeholder: "Filter notifications...",
                                }
                            }
                        }

                        // Notification list
                        div {
                            style: "flex: 1; overflow-y: auto;",

                            if filtered_notifications.is_empty() {
                                div {
                                    style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--textDim);",
                                    span { style: "font-size: 10px;", "{notif_empty_msg}" }
                                }
                            } else {
                                for n in filtered_notifications.iter() {
                                    {
                                        let type_color = match n.notif_type.as_str() {
                                            "error" | "task_error" => "var(--error)",
                                            "warning" => "var(--warning)",
                                            "success" | "task_complete" => "var(--success)",
                                            _ => "var(--accent)",
                                        };
                                        let type_bg = format!("{}22", type_color);
                                        let n_id = n.id.clone();
                                        let n_title = n.title.clone();
                                        let n_type = n.notif_type.clone();
                                        let n_msg = n.message.clone();
                                        rsx! {
                                            div {
                                                key: "{n_id}",
                                                style: "padding: 8px 12px; border-bottom: 1px solid var(--border);",

                                                div {
                                                    style: "display: flex; align-items: center; gap: 4px;",

                                                    span {
                                                        style: "font-size: 10px; font-weight: 500; color: var(--text);",
                                                        "{n_title}"
                                                    }

                                                    if !n_type.is_empty() {
                                                        span {
                                                            style: "font-size: 8px; padding: 1px 4px; border-radius: 3px; background: {type_bg}; color: {type_color};",
                                                            "{n_type}"
                                                        }
                                                    }
                                                }

                                                p {
                                                    style: "font-size: 9px; margin-top: 2px; color: var(--textDim);",
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
}

#[component]
fn StatusRow(props: StatusRowProps) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: baseline; gap: 8px;",

            span {
                style: "font-size: 9px; flex-shrink: 0; width: 64px; color: var(--textDim);",
                "{props.label}"
            }

            span {
                style: "font-size: 11px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text);",
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
