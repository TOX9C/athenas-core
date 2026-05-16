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

    let chevron_rotation = if collapsed() { 0 } else { 90 };
    let has_next_action = !eval_data.next_action.is_empty();

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
                    "Evaluation"
                }

                span {
                    style: "font-size: 9px; padding: 2px 6px; border-radius: 4px; background: {status_color}22; color: {status_color};",
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
                                _ => "var(--textDim)".to_string(),
                            };
                            rsx! {
                                div {
                                    key: "{step.step_id}",
                                    style: "display: flex; align-items: center; gap: 6px; padding: 4px 0;",

                                    span {
                                        style: "width: 6px; height: 6px; border-radius: 50%; background: {step_color}; flex-shrink: 0;",
                                    }
                                    span {
                                        style: "font-size: 11px; color: var(--text); flex: 1;",
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
