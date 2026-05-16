use crate::stores::athena::{PlanBlock, PlanStepStatus};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PlanBlockViewProps {
    pub plan: PlanBlock,
}

#[component]
pub fn PlanBlockView(props: PlanBlockViewProps) -> Element {
    let mut collapsed = use_signal(|| false);
    let plan = &props.plan;

    let completed_count = plan
        .steps
        .iter()
        .filter(|s| s.status == PlanStepStatus::Completed)
        .count();
    let total_count = plan.steps.len();
    let progress_pct = if total_count > 0 {
        (completed_count as f64 / total_count as f64 * 100.0) as u32
    } else {
        0
    };

    let status_color = match plan.status {
        crate::stores::athena::PlanStatus::Pending => "var(--textDim)",
        crate::stores::athena::PlanStatus::InProgress => "var(--accent)",
        crate::stores::athena::PlanStatus::Completed => "var(--success)",
        crate::stores::athena::PlanStatus::Failed => "var(--error)",
    };

    let chevron_rotation = if collapsed() { 0 } else { 90 };

    rsx! {
        div {
            style: "margin-top: 8px; padding: 12px; border-radius: 8px; border: 1px solid var(--border); background: var(--bgTertiary);",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px; cursor: pointer;",
                onclick: move |_| collapsed.set(!collapsed()),

                span {
                    style: "font-size: 10px; transition: transform 0.15s; transform: rotate({chevron_rotation}deg);",
                    "\u{25b6}"
                }

                span {
                    style: "font-size: 12px; font-weight: 600; color: var(--text); flex: 1;",
                    "Plan: {plan.goal}"
                }

                span {
                    style: "font-size: 9px; padding: 2px 6px; border-radius: 4px; background: {status_color}22; color: {status_color};",
                    "{completed_count}/{total_count}"
                }
            }

            // Progress bar
            div {
                style: "margin-top: 6px; width: 100%; height: 3px; border-radius: 2px; background: var(--bgTertiary); overflow: hidden;",
                div {
                    style: "width: {progress_pct}%; height: 100%; border-radius: 2px; background: {status_color}; transition: width 0.3s ease;",
                }
            }

            // Steps
            if !collapsed() {
                div {
                    style: "margin-top: 8px; display: flex; flex-direction: column; gap: 4px;",
                    for step in plan.steps.iter() {
                        {
                            let (step_label, _step_color, dot_style) = match step.status {
                                PlanStepStatus::Pending => ("", "var(--textDim)", "width: 8px; height: 8px; border-radius: 50%; border: 1.5px solid var(--textDim); background: transparent;"),
                                PlanStepStatus::InProgress => ("", "var(--accent)", "width: 8px; height: 8px; border-radius: 50%; background: var(--accent);"),
                                PlanStepStatus::Completed => ("\u{2713}", "var(--success)", "width: 8px; height: 8px; border-radius: 50%; background: var(--success);"),
                                PlanStepStatus::Failed => ("\u{2717}", "var(--error)", "width: 8px; height: 8px; border-radius: 50%; background: var(--error);"),
                            };
                            let show_agent_type = !step.agent_type.is_empty();
                            rsx! {
                                div {
                                    key: "{step.id}",
                                    style: "display: flex; align-items: center; gap: 6px; padding: 4px 0;",

                                    div {
                                        style: "{dot_style} flex-shrink: 0; display: flex; align-items: center; justify-content: center; font-size: 7px; font-weight: 700; color: #0b0e13;",
                                        "{step_label}"
                                    }
                                    span {
                                        style: "font-size: 11px; color: var(--text); flex: 1;",
                                        "{step.title}"
                                    }
                                    if show_agent_type {
                                        span {
                                            style: "font-size: 8px; padding: 1px 4px; border-radius: 3px; background: var(--bgSecondary); color: var(--textDim);",
                                            "{step.agent_type}"
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
