use crate::components::shared::icon::IconPlay;
use crate::components::shared::modal::Modal;
use crate::components::shared::segmented::Segmented;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SwarmModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn SwarmModal(props: SwarmModalProps) -> Element {
    let mut goal = use_signal(String::new);
    let mut team_size = use_signal(|| 3u8);

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

                label {
                    style: "display: flex; flex-direction: column; gap: 6px; font-size: var(--text-xs); color: var(--textMuted);",
                    "Team Size: {team_size()}"
                    Segmented {
                        options: vec!["2".to_string(), "3".to_string(), "4".to_string(), "5".to_string()],
                        selected: (team_size() as usize).saturating_sub(2),
                        on_select: move |i: usize| team_size.set((i as u8) + 2),
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
                        onclick: move |_| {
                            // TODO: launch swarm via Tauri IPC
                            props.on_close.call(());
                        },
                        IconPlay { size: Some(14), color: Some("currentColor".to_string()) }
                        "Launch"
                    }
                }
            }
        }
    }
}
