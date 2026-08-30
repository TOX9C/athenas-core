use crate::components::shared::icon::{IconCheck, IconChevronDown, IconChevronRight};
use crate::stores::athena::use_athena_store;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ThinkingProps {
    #[props(default = None)]
    pub status: Option<String>,
}

/* ─────────────────────────────────────────────────────────
 * ASSISTANT THINKING ROW
 *
 * A quiet "working" row that sits in the message
 * column like a message of its own: three typing
 * dots, a status label, and a live elapsed timer. Expandable
 * beneath it: the rolling trace of agent statuses for this
 * request — earlier steps settle into checks, the current
 * step spins.
 * ───────────────────────────────────────────────────────── */

fn format_elapsed(tenths: u64) -> String {
    let total = tenths as f64 / 10.0;
    if total < 60.0 {
        format!("{total:.1}s")
    } else {
        format!("{}m {:.1}s", (total / 60.0).floor() as u64, total % 60.0)
    }
}

#[component]
pub fn AthenaThinkingIndicator(props: ThinkingProps) -> Element {
    let status_label = props.status.as_deref().unwrap_or("Thinking");
    let mut expanded = use_signal(|| true);

    // Elapsed timer — ticks every 100ms while the indicator is mounted.
    let mut elapsed_tenths = use_signal(|| 0u64);
    let mut animation = use_future(move || async move {
        loop {
            gloo::timers::future::TimeoutFuture::new(100).await;
            elapsed_tenths.set(elapsed_tenths() + 1);
        }
    });
    use_drop(move || {
        animation.cancel();
    });

    // Rolling status trace from the store (newest last).
    let trace: Vec<String> = use_athena_store().read().streaming_trace.clone();

    rsx! {
        div {
            class: "thinking-indicator",
            style: "display: flex; align-items: flex-start; gap: 8px; padding: 6px 12px 8px;",

            div {
                style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",

                // Header — typing dots + status + elapsed + chevron.
                button {
                    type: "button",
                    aria_expanded: format!("{}", expanded()),
                    onclick: move |_| expanded.set(!expanded()),
                    style: "display: flex; align-items: center; gap: 8px; width: fit-content; padding: 2px 4px; margin: -2px -4px; border-radius: var(--radius-sm); transition: background-color 100ms var(--ease); background: transparent;",

                    // Three-dot typing indicator.
                    span {
                        class: "athena-typing-dots",
                        aria_hidden: "true",
                        for (i, delay) in [0u32, 160, 320].iter().enumerate() {
                            span {
                                key: "dot-{i}",
                                class: "athena-typing-dot",
                                style: "animation-delay: {delay}ms;",
                            }
                        }
                    }

                    // Status label.
                    span {
                        style: "font-size: var(--text-sm); font-weight: 500; white-space: nowrap; color: var(--text);",
                        "{status_label}"
                    }

                    // Live elapsed timer.
                    span {
                        style: "font-family: var(--font-mono); font-size: var(--text-xs); color: var(--textDim); font-variant-numeric: tabular-nums;",
                        "{format_elapsed(elapsed_tenths())}"
                    }

                    if expanded() {
                        IconChevronDown { size: Some(12), color: Some("var(--textDim)".to_string()) }
                    } else {
                        IconChevronRight { size: Some(12), color: Some("var(--textDim)".to_string()) }
                    }
                }

                // Expandable trace — statuses settle into checks, the newest spins.
                if expanded() && !trace.is_empty() {
                    div {
                        style: "display: flex; flex-direction: column; gap: 2px; margin-top: 2px;",
                        for (i, step) in trace.iter().enumerate() {
                            {
                                let is_last = i == trace.len() - 1;
                                let step_text = step.clone();
                                rsx! {
                                    div {
                                        key: "trace-{i}",
                                        style: "display: flex; align-items: center; gap: 8px; min-height: 20px; padding: 1px 6px; animation: fade-up 300ms var(--ease) both;",
                                        if is_last {
                                            // Spinner — current step.
                                            span {
                                                style: "width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--border); border-top-color: var(--textMuted); animation: spin 700ms linear infinite; flex-shrink: 0;",
                                            }
                                        } else {
                                            IconCheck { size: Some(10), color: Some("var(--success)".to_string()) }
                                        }
                                        span {
                                            style: "font-size: var(--text-sm); color: var(--textMuted);",
                                            "{step_text}"
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
