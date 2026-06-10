use crate::components::shared::icon::{IconCheck, IconSend};
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
            style: "margin-top: 8px; padding: 12px; border-radius: var(--radius-md); border: 1px solid var(--border); background: var(--bgTertiary);",

            // Question
            div {
                style: "font-size: 13px; color: var(--text); margin-bottom: 10px; line-height: 1.5;",
                "{ask.question}"
            }

            // Options
            if !ask.options.is_empty() {
                div {
                    style: "display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px;",
                    for option in ask.options.iter() {
                        button {
                            key: "{option.label}",
                            class: "btn-secondary btn-sm",
                            style: "text-align: left; justify-content: flex-start;",
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
                    class: "field",
                    style: "flex: 1;",
                    value: "{custom_text}",
                    oninput: move |e| custom_text.set(e.value()),
                    placeholder: "Type your response..."
                }
                button {
                    class: "btn-primary btn-sm",
                    style: "display: inline-flex; align-items: center; gap: 6px;",
                    onclick: move |_| {
                        // TODO: respond via Tauri IPC
                    },
                    IconSend { size: Some(14), color: Some("currentColor".to_string()) }
                    "Send"
                }
            }

            if ask.answered {
                div {
                    style: "margin-top: 8px; display: flex; align-items: center; gap: 6px; font-size: var(--text-xs); color: var(--success);",
                    IconCheck { size: Some(14), color: Some("var(--success)".to_string()) }
                    span { "Answered: {ask.selected_answer.as_deref().unwrap_or(\"-\")}" }
                }
            }
        }
    }
}
