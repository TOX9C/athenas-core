use crate::stores::athena::AskUserBlock;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AskUserBlockViewProps {
    pub ask: AskUserBlock,
}

#[component]
pub fn AskUserBlockView(props: AskUserBlockViewProps) -> Element {
    let mut custom_text = use_signal(String::new);
    let ask = &props.ask;

    rsx! {
        div {
            style: "margin-top: 8px; padding: 12px; border-radius: 8px; border: 1px solid var(--border); background: var(--bgTertiary);",

            // Question
            div {
                style: "font-size: 12px; color: var(--text); margin-bottom: 8px;",
                "\u{2753} {ask.question}"
            }

            // Options
            if !ask.options.is_empty() {
                div {
                    style: "display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px;",
                    for option in ask.options.iter() {
                        button {
                            key: "{option.label}",
                            style: "padding: 6px 12px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgSecondary); color: var(--text); cursor: pointer; font-size: 11px; text-align: left;",
                            onclick: move |_| {
                                // TODO: respond via Tauri IPC
                            },
                            "{option.label}"
                        }
                    }
                }
            }

            // Custom text input
            div {
                style: "display: flex; gap: 6px;",
                input {
                    style: "flex: 1; padding: 6px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 11px; outline: none;",
                    value: "{custom_text}",
                    oninput: move |e| custom_text.set(e.value()),
                    placeholder: "Type your response..."
                }
                button {
                    style: "padding: 6px 12px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px; font-weight: 600;",
                    onclick: move |_| {
                        // TODO: respond via Tauri IPC
                    },
                    "Send"
                }
            }

            if ask.answered {
                div {
                    style: "margin-top: 6px; font-size: 10px; color: var(--success);",
                    "\u{2713} Answered: {ask.selected_answer.as_deref().unwrap_or(\"-\")}"
                }
            }
        }
    }
}
