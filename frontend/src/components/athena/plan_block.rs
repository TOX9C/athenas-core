use crate::components::shared::icon::{IconCheck, IconChevronDown, IconChevronRight, IconClose};
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

    let is_collapsed = collapsed();

    rsx! {
        div {
            style: "margin-top: 8px; padding: 12px; border-radius: var(--radius-md); border: 1px solid var(--border); background: var(--bgTertiary);",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px; cursor: pointer;",
                onclick: move |_| collapsed.set(!collapsed()),

                span {
                    style: "display: inline-flex; align-items: center; color: var(--textMuted);",
                    if is_collapsed {
                        IconChevronRight { size: Some(14), color: Some("currentColor".to_string()) }
                    } else {
                        IconChevronDown { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                }

                span {
                    style: "font-family: var(--font-display); font-size: 14px; font-weight: 600; letter-spacing: 0.01em; color: var(--text); flex: 1;",
                    "Plan: {plan.goal}"
                }

                span {
                    class: "pill",
                    style: "background: {status_color}22; color: {status_color}; border-color: {status_color}44;",
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
                            let step_status = step.status.clone();
                            let show_agent_type = !step.agent_type.is_empty();
                            rsx! {
                                div {
                                    key: "{step.id}",
                                    style: "display: flex; align-items: center; gap: 8px; padding: 4px 0;",

                                    span {
                                        style: "flex-shrink: 0; width: 14px; height: 14px; display: inline-flex; align-items: center; justify-content: center;",
                                        match step_status {
                                            PlanStepStatus::Completed => rsx! {
                                                IconCheck { size: Some(13), color: Some("var(--success)".to_string()) }
                                            },
                                            PlanStepStatus::Failed => rsx! {
                                                IconClose { size: Some(13), color: Some("var(--error)".to_string()) }
                                            },
                                            PlanStepStatus::InProgress => rsx! {
                                                span { style: "width: 8px; height: 8px; border-radius: 50%; background: var(--accent);" }
                                            },
                                            PlanStepStatus::Pending => rsx! {
                                                span { style: "width: 8px; height: 8px; border-radius: 50%; border: 1.5px solid var(--textDim); background: transparent;" }
                                            },
                                        }
                                    }
                                    span {
                                        style: "font-size: 12px; color: var(--text); flex: 1;",
                                        "{step.title}"
                                    }
                                    if show_agent_type {
                                        span {
                                            class: "badge",
                                            style: "color: var(--textMuted);",
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
