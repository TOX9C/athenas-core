use crate::components::shared::icon::{IconPlay, IconSwarm};
use crate::components::shared::modal::Modal;
use crate::stores::ui::use_ui_store;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SwarmModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn SwarmModal(props: SwarmModalProps) -> Element {
    let mut goal = use_signal(String::new);
    let mut ui_state = use_ui_store();

    rsx! {
        Modal {
            title: "Launch Swarm",
            on_close: move |_| props.on_close.call(()),
            width: 440,
            compact: true,

            div {
                class: "swarm-launch-form",
                style: "display: flex; flex-direction: column; gap: 14px;",

                div {
                    class: "swarm-launch-intro",
                    style: "display: flex; align-items: flex-start; gap: 10px;",
                    span {
                        style: "display: inline-flex; align-items: center; justify-content: center; width: 30px; height: 30px; flex-shrink: 0; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--accentSubtle); color: var(--accent);",
                        IconSwarm { size: Some(16), color: Some("currentColor".to_string()) }
                    }
                    div {
                        strong { style: "display: block; color: var(--text); font-size: var(--text-md);", "Give the team one clear outcome" }
                        span { style: "display: block; margin-top: 2px; color: var(--textMuted); font-size: var(--text-sm); line-height: 1.45;", "Athena will coordinate agents around this goal." }
                    }
                }

                label {
                    style: "display: flex; flex-direction: column; gap: 6px; font-size: var(--text-xs); color: var(--textMuted);",
                    "Mission goal"
                    textarea {
                        class: "field swarm-goal-field",
                        style: "min-height: 92px; resize: vertical;",
                        value: "{goal}",
                        oninput: move |e| goal.set(e.value()),
                        placeholder: "For example: review the auth flow, fix the failing tests, and summarize the changes."
                    }
                }

                div {
                    style: "display: flex; justify-content: flex-end; gap: 8px; margin-top: 2px;",

                    button {
                        class: "btn-ghost",
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn-primary",
                        style: "display: inline-flex; align-items: center; gap: 6px;",
                        disabled: goal().trim().is_empty(),
                        onclick: move |_| {
                            let goal_text = goal();
                            if goal_text.trim().is_empty() { return; }
                            ui_state.write().pending_swarm_goal = Some(goal_text);
                            ui_state.write().show_new_space_modal = true;
                            ui_state.write().show_swarm_modal = false;
                        },
                        IconPlay { size: Some(14), color: Some("currentColor".to_string()) }
                        "Launch"
                    }
                }
            }
        }
    }
}
