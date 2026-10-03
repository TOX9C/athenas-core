use super::agent_output_panel::AgentOutputPanel;
use super::agent_selector::AgentSelector;
use super::agent_status_bar::{pane_status_of, status_dot_color, AgentPaneStatus};
use crate::components::shared::icon::{IconBell, IconClose, IconPulse, IconSearch, IconTerminal};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::use_agent_status_store;
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
    let agent_output = use_agent_output_store();
    let agent_output_memo = agent_output.clone();
    let agent_output_select = agent_output.clone();
    let agent_output_close = agent_output.clone();
    let agent_status = use_agent_status_store();
    let notifications = use_notification_store();

    let mut tab = use_signal(InspectorTab::default);
    let mut search_query = use_signal(String::new);

    // Memoized lookups: these would otherwise re-scan and re-lowercase every
    // notification on EVERY store write (any re-render of this component).
    let selected_pane_id =
        use_memo(move || agent_output_memo.selected_pane_id_signal().read().clone());
    let pane_status: Option<AgentPaneStatus> = {
        let selected = selected_pane_id.read().clone();
        selected.as_ref().and_then(|id| {
            agent_status
                .status_signal(id)
                .map(|status| pane_status_of(&status.read()))
        })
    };

    // History backfill: while the inspector is closed, output-capture work is
    // gated off (see `AgentOutputStore::capture_wanted`), so this pane's
    // frontend buffer holds no lines from the closed period. On open — and on
    // pane selection change while open — seed the buffer from the backend's
    // always-maintained OutputBuffer tail (240 lines, same bound as the
    // Athena context snapshot). Seeding is prepend-only by line number, so
    // live batches that raced the fetch are never clobbered.
    let agent_output_seed = agent_output.clone();
    use_effect(move || {
        let open = *agent_output_seed.inspector_open_signal().read();
        let selected = agent_output_seed.selected_pane_id_signal().read().clone();
        if !open {
            return;
        }
        let Some(pane_id) = selected else { return };
        let store = agent_output_seed.clone();
        spawn(async move {
            match crate::tauri_bridge::get_pane_history(&pane_id, 240).await {
                Ok(history) => {
                    let lines: Vec<crate::stores::agent_output::OutputLine> = history
                        .into_iter()
                        .map(|line| crate::stores::agent_output::OutputLine {
                            is_stderr: crate::stores::agent_output::is_stderr_like(&line.text),
                            text: std::rc::Rc::from(line.text.as_str()),
                            pane_id: line.pane_id,
                            line_num: line.line_num as usize,
                            timestamp: line.timestamp as i64,
                        })
                        .collect();
                    if !lines.is_empty() {
                        store.seed_history(&pane_id, lines);
                    }
                }
                Err(error) => {
                    web_sys::console::warn_1(
                        &format!(
                            "[AgentInspector] history backfill failed for pane {pane_id}: {error:?}"
                        )
                        .into(),
                    );
                }
            }
        });
    });

    let filtered_notifications: Vec<NotificationItem> = use_memo(move || {
        let pane_filter = selected_pane_id.read().clone();
        let query = search_query.read().to_lowercase();
        notifications
            .read()
            .iter()
            .filter(|n| match &pane_filter {
                Some(pane_id) => n.source == *pane_id,
                None => true,
            })
            .filter(|n| {
                query.is_empty()
                    || n.title.to_lowercase().contains(&query)
                    || n.message.to_lowercase().contains(&query)
            })
            .map(to_notif_item)
            .collect()
    })();

    let inspector_open = *agent_output.inspector_open_signal().read();
    if !inspector_open {
        return rsx! {};
    }

    // Tab definitions: (tab enum, label)
    let tabs = [
        (InspectorTab::Output, "Output"),
        (InspectorTab::Status, "Status"),
        (InspectorTab::Notifications, "Alerts"),
    ];

    let (notif_empty_title, notif_empty_hint) = if selected_pane_id.read().is_some() {
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
                        agent_output_select.select_agent(Some(id));
                    }
                }

                div { style: "flex: 1;" }

                button {
                    class: "icon-btn",
                    title: "Close inspector",
                    onclick: move |_| agent_output_close.set_inspector_open(false),
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
