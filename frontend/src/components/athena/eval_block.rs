use crate::components::shared::icon::{IconChevronDown, IconChevronRight};
use crate::stores::athena::EvaluationBlock;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct EvaluationBlockViewProps {
    pub eval: EvaluationBlock,
}

#[component]
pub fn EvaluationBlockView(props: EvaluationBlockViewProps) -> Element {
    let mut collapsed = use_signal(|| false);
    let eval_data = &props.eval;

    let status_color = match eval_data.overall_status.as_str() {
        "completed" | "success" => "var(--success)",
        "failed" | "error" => "var(--error)",
        "in_progress" => "var(--accent)",
        _ => "var(--textDim)",
    };

    let is_collapsed = collapsed();
    let has_next_action = !eval_data.next_action.is_empty();

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
                    "Evaluation"
                }

                span {
                    class: "pill",
                    style: "background: {status_color}22; color: {status_color}; border-color: {status_color}44;",
                    "{eval_data.overall_status}"
                }
            }

            if !collapsed() {
                div {
                    style: "margin-top: 8px; display: flex; flex-direction: column; gap: 4px;",

                    for step in eval_data.step_evaluations.iter() {
                        {
                            let step_color = match step.status.as_str() {
                                "completed" | "success" => "var(--success)".to_string(),
                                "failed" | "error" => "var(--error)".to_string(),
                                "warning" | "partial" => "var(--warning)".to_string(),
                                _ => "var(--textDim)".to_string(),
                            };
                            rsx! {
                                div {
                                    key: "{step.step_id}",
                                    style: "display: flex; align-items: center; gap: 8px; padding: 4px 0;",

                                    span {
                                        style: "width: 7px; height: 7px; border-radius: 50%; background: {step_color}; flex-shrink: 0;",
                                    }
                                    span {
                                        style: "font-size: 12px; color: var(--text); flex: 1;",
                                        "{step.summary}"
                                    }
                                }
                            }
                        }
                    }

                    if has_next_action {
                        div {
                            style: "margin-top: 4px; font-size: 10px; color: var(--accent);",
                            "Next: {eval_data.next_action}"
                        }
                    }
                }
            }
        }
    }
}
