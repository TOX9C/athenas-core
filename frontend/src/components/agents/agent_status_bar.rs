use crate::components::shared::icon::IconHelmet;
use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus, AgentStatus};
use dioxus::prelude::*;

/// Status info for an agent pane.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentPaneStatus {
    pub pane_id: String,
    pub status: String,
    pub agent_type: String,
    pub message: String,
    pub progress: Option<ProgressInfo>,
    pub last_updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProgressInfo {
    pub current: usize,
    pub total: usize,
    pub label: Option<String>,
}

/// Get the (short, readable) status labels and color for a given status string.
fn status_label(status: &str) -> (&'static str, &'static str, &'static str) {
    match status {
        "thinking" | "working" => ("RUN", "Running", "var(--accent)"),
        "waiting_for_input" => ("WAIT", "Waiting", "var(--warning)"),
        "completed" => ("OK", "Done", "var(--success)"),
        "error" => ("ERR", "Error", "var(--error)"),
        "cancelled" => ("CX", "Cancelled", "var(--textDim)"),
        "disconnected" => ("OFF", "Offline", "var(--textDim)"),
        "idle" => ("IDLE", "Idle", "var(--textDim)"),
        _ => ("???", "Unknown", "var(--textDim)"),
    }
}

/// Get a color for an agent type.
fn get_agent_color(agent_type: &str) -> &'static str {
    match agent_type {
        "claude" => "#f97316",
        "codex" => "#10b981",
        "opencode" => "#8b5cf6",
        "gemini" => "#3b82f6",
        "shell" => "#6b7280",
        _ => "var(--accent)",
    }
}

fn time_ago(ts: i64) -> String {
    let now_ms = js_sys::Date::now() as i64;
    let diff_secs = ((now_ms - ts) / 1000).max(0) as u64;
    if diff_secs < 5 {
        return "now".to_string();
    }
    if diff_secs < 60 {
        return format!("{}s", diff_secs);
    }
    if diff_secs < 3600 {
        return format!("{}m", diff_secs / 60);
    }
    format!("{}h", diff_secs / 3600)
}

/// Convert store AgentStatus to the component-level AgentPaneStatus.
fn to_pane_status(agent_status: &AgentStatus) -> AgentPaneStatus {
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
            label: if p.label.is_empty() {
                None
            } else {
                Some(p.label.clone())
            },
        }),
        last_updated_at: agent_status.last_updated_at,
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct AgentStatusBarProps {
    pub pane_id: String,
}

#[component]
pub fn AgentStatusBar(props: AgentStatusBarProps) -> Element {
    let agent_status = use_agent_status_store();
    let agent_output = use_agent_output_store();

    let current_status: AgentPaneStatus = agent_status
        .read()
        .statuses
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| to_pane_status(s))
        .unwrap_or_default();

    let line_count: usize = agent_output
        .read()
        .buffers
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, lines)| lines.len())
        .unwrap_or(0);

    let (label, word, color) = status_label(&current_status.status);
    let agent_color = get_agent_color(&current_status.agent_type);
    let is_spinning = matches!(current_status.status.as_str(), "thinking" | "working");
    let dot_class = if is_spinning { "pulse-soft" } else { "" };
    let display_id: String = props.pane_id.chars().take(10).collect();
    let msg_preview: String = current_status.message.chars().take(40).collect();
    let ago = time_ago(current_status.last_updated_at);

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-top: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0; overflow-x: hidden;",

            // Agent helmet glyph
            span {
                style: "display: inline-flex; align-items: center; color: {agent_color}; flex-shrink: 0;",
                IconHelmet { size: Some(14), color: Some("currentColor".to_string()) }
            }

            // Status chip — dot + readable word
            span {
                style: "display: inline-flex; align-items: center; gap: 5px; padding: 1px 8px; border-radius: var(--radius-pill); background: var(--bgTertiary); flex-shrink: 0;",
                div {
                    class: "{dot_class}",
                    style: "width: 7px; height: 7px; border-radius: var(--radius-pill); background: {color}; flex-shrink: 0;",
                }
                span {
                    style: "font-size: var(--text-2xs); font-weight: 600; color: {color}; letter-spacing: 0.02em;",
                    title: "{label}",
                    "{word}"
                }
            }

            // Pane id
            span {
                style: "font-size: var(--text-2xs); font-family: var(--fontFamily); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--textMuted); flex-shrink: 0;",
                "{display_id}"
            }

            // Message preview
            if !current_status.message.is_empty() {
                span {
                    style: "font-size: var(--text-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; color: var(--textDim);",
                    "{msg_preview}"
                }
            }

            // Line count
            if line_count > 0 {
                span {
                    style: "font-size: var(--text-2xs); flex-shrink: 0; color: var(--textDim);",
                    "{line_count} lines"
                }
            }

            // Time ago
            span {
                style: "font-size: var(--text-2xs); flex-shrink: 0; color: var(--textDim);",
                "{ago}"
            }
        }
    }
}
