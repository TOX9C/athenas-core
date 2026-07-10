use dioxus::prelude::*;

/// A single output line from an agent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputLine {
    pub pane_id: String,
    pub line_num: usize,
    pub text: String,
    pub timestamp: i64,
    pub is_stderr: bool,
}

/// Format a unix-ms timestamp as HH:MM:SS.
fn format_time(ts: i64) -> String {
    // Simple formatting — in WASM we avoid chrono timezone features
    let secs = (ts / 1000) % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[derive(Props, Clone, PartialEq)]
pub struct AgentOutputLineProps {
    pub line: OutputLine,
    #[props(default = false)]
    pub show_line_numbers: bool,
}

#[component]
pub fn AgentOutputLine(props: AgentOutputLineProps) -> Element {
    // `is_stderr` is precomputed at line arrival (see
    // `stores::agent_output::is_stderr_like`); no per-render allocation.
    let is_err = props.line.is_stderr;
    let color = if is_err {
        "var(--error)"
    } else {
        "var(--textMuted)"
    };
    let row_bg = if is_err {
        "background: color-mix(in srgb, var(--error) 7%, transparent);"
    } else {
        ""
    };

    rsx! {
        div {
            class: "lit-sweep",
            style: "display: flex; align-items: flex-start; gap: 8px; padding: 0 8px; font-family: var(--fontFamily); font-size: var(--text-xs); line-height: 1.6; color: {color}; {row_bg}",

            if props.show_line_numbers {
                span {
                    style: "flex-shrink: 0; text-align: right; width: 36px; color: var(--textDim); opacity: 0.5; user-select: none; font-family: var(--fontFamily);",
                    "{props.line.line_num}"
                }
            }

            span {
                style: "flex-shrink: 0; user-select: none; color: var(--textDim); font-size: var(--text-2xs); opacity: 0.6;",
                "{format_time(props.line.timestamp)}"
            }

            span {
                style: "flex: 1; min-width: 0; white-space: pre-wrap; word-break: break-all;",
                "{props.line.text}"
            }
        }
    }
}
