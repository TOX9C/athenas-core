use dioxus::prelude::*;

/// Represents a command execution with its output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandBlockData {
    pub id: String,
    pub command: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub started_at: i64,
    pub duration_ms: Option<u64>,
}

#[derive(Props, Clone, PartialEq)]
pub struct CommandBlockProps {
    pub block: CommandBlockData,
}

#[component]
pub fn CommandBlock(props: CommandBlockProps) -> Element {
    let mut collapsed = use_signal(|| false);
    let block = &props.block;

    let exit_color = match block.exit_code {
        Some(0) => "var(--success)",
        Some(_) => "var(--error)",
        None => "var(--textDim)",
    };

    let exit_label = match block.exit_code {
        Some(0) => "0".to_string(),
        Some(c) => format!("{}", c),
        None => "running".to_string(),
    };

    let duration_str = block.duration_ms.map_or("...".to_string(), |d| {
        if d < 1000 {
            format!("{}ms", d)
        } else {
            format!("{:.1}s", d as f64 / 1000.0)
        }
    });

    let chevron_rotation: i32 = if collapsed() { 0 } else { 90 };
    let exit_color_bg = format!("{}22", exit_color);

    rsx! {
        div {
            class: "command-block",
            style: "border: 1px solid var(--border); border-radius: 6px; overflow: hidden; margin-bottom: 4px;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 6px; padding: 6px 10px; background: var(--bgSecondary); cursor: pointer;",
                onclick: move |_| collapsed.set(!collapsed()),

                span {
                    style: "font-size: 10px; transition: transform 0.15s; transform: rotate({chevron_rotation}deg); color: var(--textDim);",
                    "\u{25b6}"
                }

                code {
                    style: "font-size: 11px; color: var(--text); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "$ {block.command}"
                }

                span {
                    style: "font-size: 9px; padding: 1px 5px; border-radius: 3px; background: {exit_color_bg}; color: {exit_color};",
                    "{exit_label}"
                }

                span {
                    style: "font-size: 9px; color: var(--textDim);",
                    "{duration_str}"
                }
            }

            // Output
            if !collapsed() {
                div {
                    style: "padding: 8px 10px; font-family: monospace; font-size: 11px; color: var(--textMuted); background: var(--bg); max-height: 200px; overflow-y: auto; white-space: pre-wrap;",
                    "{block.output}"
                }
            }
        }
    }
}
