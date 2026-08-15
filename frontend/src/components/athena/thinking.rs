use crate::components::shared::icon::{IconCheck, IconChevronDown, IconChevronRight};
use crate::stores::athena::use_athena_store;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ThinkingProps {
    #[props(default = None)]
    pub status: Option<String>,
}

/* ─────────────────────────────────────────────────────────
 * PIXEL-GRID LOADER + AGENT TRACE
 *
 * Header row: a 3×3 chevron wavefront of gold cells, a
 * shimmering status label, and a live elapsed timer in mono
 * tabular figures. Expandable beneath it: the rolling trace
 * of agent statuses for this request — earlier steps settle
 * into checks, the current step spins. The reduced-motion
 * guard in styles.css freezes the grid to its dim state.
 * ───────────────────────────────────────────────────────── */

/// Chevron wavefront delays, one per grid cell. The 650ms cycle is shorter
/// than the sweep, so two fronts are always in flight.
const PIXEL_DELAYS: [u32; 9] = [90, 180, 270, 0, 90, 180, 90, 180, 270];

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
            style: "display: flex; flex-direction: column; gap: 4px; padding: 8px 12px;",

            // Header — pixel grid + shimmer label + elapsed + chevron.
            button {
                type: "button",
                aria_expanded: format!("{}", expanded()),
                onclick: move |_| expanded.set(!expanded()),
                style: "display: flex; align-items: center; gap: 8px; width: fit-content; padding: 2px 4px; margin: -2px -4px; border-radius: var(--radius-sm); transition: background-color 100ms var(--ease); background: transparent;",

                // 3×3 pixel grid — gold cells driven by a chevron wavefront.
                span {
                    aria_hidden: "true",
                    style: "display: grid; grid-template-columns: repeat(3, 4px); gap: 1.5px;",
                    for (i, delay) in PIXEL_DELAYS.iter().enumerate() {
                        span {
                            key: "px-{i}",
                            style: "width: 4px; height: 4px; border-radius: 1px; background: var(--accent); opacity: 0.15; animation: pixel-on 650ms ease-in-out {delay}ms infinite;",
                        }
                    }
                }

                // Shimmering status label.
                span {
                    class: "shimmer-text",
                    style: "font-size: var(--text-sm); font-weight: 500; white-space: nowrap;",
                    "{status_label}"
                }

                // Live elapsed timer — mono tabular figures.
                span {
                    style: "font-family: var(--font-mono); font-size: var(--text-xs); color: var(--textDim); font-variant-numeric: tabular-nums;",
                    "{format_elapsed(elapsed_tenths())}"
                }

                if expanded() {
                    IconChevronDown { size: Some(13), color: Some("var(--textDim)".to_string()) }
                } else {
                    IconChevronRight { size: Some(13), color: Some("var(--textDim)".to_string()) }
                }
            }

            // Expandable trace — statuses settle into checks, the newest spins.
            if expanded() && !trace.is_empty() {
                div {
                    style: "display: flex; flex-direction: column; gap: 3px; margin-top: 4px; padding-left: 2px;",
                    for (i, step) in trace.iter().enumerate() {
                        {
                            let is_last = i == trace.len() - 1;
                            let step_text = step.clone();
                            rsx! {
                                div {
                                    key: "trace-{i}",
                                    style: format!(
                                        "display: flex; align-items: center; gap: 8px; min-height: 20px; padding: 1px 6px; animation: fade-up 320ms var(--ease) {}ms both;",
                                        i * 90
                                    ),
                                    if is_last {
                                        // Spinner — current step.
                                        span {
                                            style: "width: 10px; height: 10px; border-radius: 50%; border: 1.5px solid var(--border); border-top-color: var(--textMuted); animation: spin 700ms linear infinite; flex-shrink: 0;",
                                        }
                                    } else {
                                        IconCheck { size: Some(11), color: Some("var(--success)".to_string()) }
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
