use crate::components::shared::icon::{IconCheck, IconSend};
use crate::stores::athena::{use_athena_store, AskUserBlock};
use crate::tauri_bridge;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AskUserBlockViewProps {
    pub ask: AskUserBlock,
}

#[component]
pub fn AskUserBlockView(props: AskUserBlockViewProps) -> Element {
    let mut custom_text = use_signal(String::new);
    let athena = use_athena_store();
    let ask = &props.ask;
    let request_id = ask.request_id.clone();

    rsx! {
        div {
            style: "margin-top: 8px; padding: 12px; border-radius: var(--radius-md); border: none; background: transparent;",

            // Question
            div {
                style: "font-family: var(--font-display); font-size: 13px; color: var(--accent); margin-bottom: 10px; line-height: 1.5; letter-spacing: 0.02em;",
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
                            onclick: {
                                let request_id = request_id.clone();
                                let label = option.label.clone();
                                move |_| {
                                    let req_id = request_id.clone();
                                    let resp = label.clone();
                                    let mut ath = athena;
                                    spawn(async move {
                                        let _ = tauri_bridge::athena_user_answer(&req_id, &resp).await;
                                        ath.write().mark_ask_user_answered(&req_id, &resp);
                                    });
                                }
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
                    onclick: {
                        let request_id = request_id.clone();
                        move |_| {
                            let text = custom_text.read().clone();
                            if text.is_empty() { return; }
                            let req_id = request_id.clone();
                            let mut ath = athena;
                            spawn(async move {
                                let _ = tauri_bridge::athena_user_answer(&req_id, &text).await;
                                ath.write().mark_ask_user_answered(&req_id, &text);
                            });
                            custom_text.set(String::new());
                        }
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
