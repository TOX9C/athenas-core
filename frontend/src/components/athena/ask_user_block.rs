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
    let mut selected = use_signal(|| None::<usize>);
    let athena = use_athena_store();
    // Owned clone so 'static move-closures (event handlers) can read fields
    // without borrowing into `props`.
    let ask = props.ask.clone();
    let request_id = ask.request_id.clone();
    let options: Vec<crate::stores::athena::AskUserOption> = ask.options.clone();

    let has_answer = selected().is_some() || !custom_text.read().trim().is_empty();
    let send_ready = has_answer && !ask.answered;

    let answered_text = ask.selected_answer.clone().unwrap_or_default();

    // Submit an answer to the backend and settle the block into its
    // answered state. A move-closure over Copy values only, so it can be
    // copied into every handler; each handler clones its own strings.
    let submit = move |request_id: String, response: String| {
        let req_id = request_id.clone();
        let resp = response.clone();
        let mut ath = athena;
        spawn(async move {
            let _ = tauri_bridge::athena_user_answer(&req_id, &resp).await;
            ath.write().mark_ask_user_answered(&req_id, &resp);
        });
    };

    // Confirm the current selection / custom text.
    let confirm = {
        let request_id = request_id.clone();
        let options = options.clone();
        move |_| {
            if let Some(i) = selected() {
                if let Some(option) = options.get(i) {
                    submit(request_id.clone(), option.label.clone());
                }
            } else {
                let text = custom_text.read().clone();
                if !text.trim().is_empty() {
                    submit(request_id.clone(), text.trim().to_string());
                    custom_text.set(String::new());
                }
            }
        }
    };

    rsx! {
        div {
            style: "margin-top: 6px; border-radius: var(--radius-md); border: 1px solid var(--border); background: var(--bgSecondary); overflow: hidden; animation: fade-up 300ms var(--ease) both;",

            if ask.answered {
                // ── Answered — a slim recap row, not a full card. ──
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 10px 12px;",
                    span {
                        style: "width: 18px; height: 18px; border-radius: 50%; background: var(--success); color: var(--bg); display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; animation: pop-in 260ms cubic-bezier(0.22,0.61,0.36,1) both;",
                        IconCheck { size: Some(10), color: Some("var(--bg)".to_string()) }
                    }
                    span {
                        style: "font-size: var(--text-xs); color: var(--textDim);",
                        "You chose"
                    }
                    span {
                        style: "font-size: var(--text-base); font-weight: 500; color: var(--text); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{answered_text}"
                    }
                }
            } else {
                // ── Question — prompt + selectable options + custom answer. ──
                div {
                    style: "padding: 12px 12px 6px;",
                    span {
                        style: "display: block; font-size: var(--text-base); font-weight: 500; color: var(--text); line-height: 1.5;",
                        "{ask.question}"
                    }
                }

                if !options.is_empty() {
                    div {
                        style: "padding: 4px 10px 0; display: flex; flex-direction: column; gap: 2px;",
                        for (i, option) in options.iter().enumerate() {
                            {
                                let on = selected() == Some(i);
                                let label = option.label.clone();
                                let description = option.description.clone();
                                rsx! {
                                    button {
                                        key: "opt-{i}",
                                        type: "button",
                                        aria_pressed: format!("{}", on),
                                        class: "athena-ask-option",
                                        onclick: move |_| {
                                            // Select only — the send button confirms.
                                            selected.set(Some(i));
                                            custom_text.set(String::new());
                                        },
                                        style: format!(
                                            "display: flex; align-items: center; gap: 10px; width: 100%; text-align: left; padding: 7px 8px; border-radius: var(--radius-sm); transition: background-color 100ms var(--ease); {}",
                                            if on {
                                                "background: var(--accentSubtle); box-shadow: inset 0 0 0 1px var(--accent);"
                                            } else {
                                                ""
                                            }
                                        ),
                                        span {
                                            style: format!(
                                                "width: 16px; height: 16px; border-radius: 50%; flex-shrink: 0; display: inline-flex; align-items: center; justify-content: center; transition: background-color 200ms var(--ease), box-shadow 200ms var(--ease); {}",
                                                if on { "background: var(--accent);" } else { "box-shadow: inset 0 0 0 1.5px var(--textDim);" }
                                            ),
                                            if on {
                                                span {
                                                    style: "width: 6px; height: 6px; border-radius: 50%; background: var(--bgSecondary); animation: pop-in 200ms var(--ease) both;",
                                                }
                                            }
                                        }
                                        span {
                                            style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                                            span {
                                                style: "font-size: var(--text-base); color: var(--text); line-height: 1.35;",
                                                "{label}"
                                            }
                                            if !description.is_empty() {
                                                span {
                                                    style: "font-size: var(--text-xs); color: var(--textDim); line-height: 1.35;",
                                                    "{description}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Custom answer row.
                div {
                    style: "padding: 2px 10px 0;",
                    label {
                        class: "athena-ask-option",
                        style: "display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 8px; border-radius: var(--radius-sm); transition: background-color 100ms var(--ease);",
                        span { aria_hidden: "true", style: "width: 16px; height: 16px; flex-shrink: 0;" }
                        input {
                            value: "{custom_text}",
                            oninput: move |event| {
                                custom_text.set(event.value());
                                selected.set(None);
                            },
                            onkeydown: {
                                let request_id = request_id.clone();
                                move |event| {
                                    if event.key() == Key::Enter && !custom_text.read().trim().is_empty() && !ask.answered {
                                        let text = custom_text.read().clone();
                                        let resp = text.trim().to_string();
                                        submit(request_id.clone(), resp);
                                        custom_text.set(String::new());
                                    }
                                }
                            },
                            placeholder: "Type a custom answer…",
                            aria_label: "Custom answer",
                            style: "min-width: 0; flex: 1; background: transparent; border: none; outline: none; color: var(--text); font-family: var(--font-ui); font-size: var(--text-base); padding: 1px 0;",
                        }
                    }
                }

                // Footer — send affordance fills with gold once answerable.
                div {
                    style: "display: flex; align-items: center; justify-content: flex-end; padding: 6px 12px 10px;",
                    button {
                        type: "button",
                        class: "athena-ask-send",
                        aria_label: "Send answer",
                        title: "Send answer (Enter)",
                        disabled: !send_ready,
                        onclick: confirm,
                        style: if send_ready {
                            "background: var(--accent); color: var(--bg); box-shadow: inset 0 1px 0 rgba(255,255,255,0.14);".to_string()
                        } else {
                            "background: var(--bgTertiary); color: var(--textDim);".to_string()
                        },
                        IconSend { size: Some(13), color: Some("currentColor".to_string()) }
                    }
                }
            }
        }
    }
}
