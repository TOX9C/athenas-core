use super::athena_input::AthenaInput;
use super::chat_message::AthenaChatMessage;
use super::session_list::SessionList;
use super::thinking::AthenaThinkingIndicator;
use crate::stores::athena::{
    use_athena_store, AskUserOption, PlanStatus, PlanStepStatus, StepEvaluation,
};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[component]
pub fn AthenaPanel() -> Element {
    let athena_state = use_athena_store();
    let mut show_sessions = use_signal(|| false);
    let mut mounted = use_signal(|| false);

    // Register Tauri event listeners on mount.
    use_effect(move || {
        if mounted() {
            return;
        }
        mounted.set(true);

        let store = athena_state;

        // athena:status — Update thinking/working/idle state.
        let mut status_store = store;
        let _ = tauri_bridge::listen("athena:status", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("idle");
                let detail = val
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                status_store.write().handle_status_event(status, detail);
            }
        });

        // athena:askUser — Show interactive user question modal.
        let mut ask_store = store;
        let _ = tauri_bridge::listen("athena:askUser", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let request_id = val
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let question = val
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let options: Vec<AskUserOption> = val
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|o| {
                                let label = o.get("label").and_then(|v| v.as_str())?.to_string();
                                let description = o
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Some(AskUserOption { label, description })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ask_store.write().handle_ask_user(request_id, question, options);
            }
        });

        // athena:planUpdate — Update plan display.
        let mut plan_store = store;
        let _ = tauri_bridge::listen("athena:planUpdate", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let plan_id = val
                    .get("planId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let goal = val
                    .get("goal")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let status_str = val.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
                let status = match status_str {
                    "in_progress" => PlanStatus::InProgress,
                    "completed" => PlanStatus::Completed,
                    "failed" => PlanStatus::Failed,
                    _ => PlanStatus::Pending,
                };
                let steps: Vec<_> = val
                    .get("steps")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| {
                                let id = s.get("id").and_then(|v| v.as_str())?.to_string();
                                let title = s.get("title").and_then(|v| v.as_str())?.to_string();
                                let description = s
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let agent_type = s
                                    .get("agentType")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let step_status_str =
                                    s.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
                                let step_status = match step_status_str {
                                    "in_progress" => PlanStepStatus::InProgress,
                                    "completed" => PlanStepStatus::Completed,
                                    "failed" => PlanStepStatus::Failed,
                                    _ => PlanStepStatus::Pending,
                                };
                                Some(crate::stores::athena::PlanStepBlock {
                                    id,
                                    title,
                                    description,
                                    agent_type,
                                    status: step_status,
                                    assigned_pane_id: s
                                        .get("assignedPaneId")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    result_summary: s
                                        .get("resultSummary")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                plan_store
                    .write()
                    .handle_plan_update(plan_id, goal, steps, status);
            }
        });

        // athena:planEvaluated — Show evaluation results.
        let mut eval_store = store;
        let _ = tauri_bridge::listen("athena:planEvaluated", move |payload: String| {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                let plan_id = val
                    .get("planId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let overall_status = val
                    .get("overallStatus")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let next_action = val
                    .get("nextAction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let reasoning = val
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let step_evaluations: Vec<StepEvaluation> = val
                    .get("stepEvaluations")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| {
                                let step_id =
                                    s.get("stepId").and_then(|v| v.as_str())?.to_string();
                                let status =
                                    s.get("status").and_then(|v| v.as_str())?.to_string();
                                let summary = s
                                    .get("summary")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Some(StepEvaluation {
                                    step_id,
                                    status,
                                    summary,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                eval_store.write().handle_plan_evaluated(
                    plan_id,
                    overall_status,
                    step_evaluations,
                    next_action,
                    reasoning,
                );
            }
        });
    });

    let state = athena_state.read();

    let model_label = if state.model.is_empty() {
        "claude".to_string()
    } else {
        state.model.clone()
    };

    rsx! {
        div {
            class: "athena-panel",
            style: "display: flex; flex-direction: row; height: 100%; background: var(--bg); color: var(--text);",

            // Session list sidebar (toggle)
            if show_sessions() {
                div {
                    style: "width: 180px; min-width: 180px; border-right: 1px solid var(--border); background: var(--bgSecondary); display: flex; flex-direction: column;",
                    SessionList {}
                }
            }

            // Main chat area
            div {
                style: "flex: 1; display: flex; flex-direction: column; min-width: 0;",

                // Header
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",

                    button {
                        style: "padding: 4px 8px; border-radius: 4px; border: none; background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 11px;",
                        onclick: move |_| show_sessions.set(!show_sessions()),
                        if show_sessions() { "\u{00d7}" } else { "\u{2630}" }
                    }

                    span {
                        style: "font-size: 13px; font-weight: 600; color: var(--text); flex: 1;",
                        "Athena"
                    }

                    span {
                        style: "font-size: 10px; padding: 2px 8px; border-radius: 9999px; background: var(--bgTertiary); color: var(--accent);",
                        "{model_label}"
                    }

                    if state.is_streaming {
                        span {
                            style: "font-size: 9px; color: var(--accent);",
                            "streaming..."
                        }
                    }
                }

                // Messages
                div {
                    style: "flex: 1; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 8px;",

                    if state.messages.is_empty() {
                        div {
                            style: "flex: 1; display: flex; align-items: center; justify-content: center; color: var(--textDim);",
                            div {
                                style: "text-align: center;",
                                span {
                                    style: "font-size: 24px; font-weight: 700; opacity: 0.25; display: block; color: var(--accent);",
                                    "A"
                                }
                                span { style: "font-size: 12px; margin-top: 8px; display: block;", "Ask Athena anything..." }
                            }
                        }
                    } else {
                        for msg in state.messages.iter() {
                            AthenaChatMessage { key: "{msg.id}", message: msg.clone() }
                        }
                    }

                    if state.is_streaming {
                        AthenaThinkingIndicator { status: state.streaming_status.clone() }
                    }
                }

                // Input
                AthenaInput {}
            }
        }
    }
}
