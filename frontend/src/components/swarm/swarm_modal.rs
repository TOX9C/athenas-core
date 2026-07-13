use crate::components::shared::icon::IconPlay;
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

            div {
                style: "display: flex; flex-direction: column; gap: 16px;",

                label {
                    style: "display: flex; flex-direction: column; gap: 6px; font-size: var(--text-xs); color: var(--textMuted);",
                    "Goal"
                    textarea {
                        class: "field",
                        style: "min-height: 84px; resize: vertical;",
                        value: "{goal}",
                        oninput: move |e| goal.set(e.value()),
                        placeholder: "What should the swarm accomplish?"
                    }
                }

                div {
                    style: "display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px;",

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
