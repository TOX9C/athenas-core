use crate::components::shared::modal::Modal;
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
                style: "display: flex; flex-direction: column; gap: 12px;",

                label {
                    style: "font-size: 11px; color: var(--text);",
                    "Goal"
                    textarea {
                        style: "width: 100%; padding: 8px 12px; margin-top: 4px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; outline: none; resize: vertical; min-height: 80px; box-sizing: border-box;",
                        value: "{goal}",
                        oninput: move |e| goal.set(e.value()),
                        placeholder: "What should the swarm accomplish?"
                    }
                }

                label {
                    style: "font-size: 11px; color: var(--text);",
                    "Team Size: {team_size()}"
                    div {
                        style: "display: flex; gap: 6px; margin-top: 4px;",

                        for size in [2u8, 3, 4, 5] {
                            {
                                let is_sel = team_size() == size;
                                let bg = if is_sel { "var(--accent)" } else { "var(--bgTertiary)" };
                                let color = if is_sel { "#0b0e13" } else { "var(--text)" };
                                rsx! {
                                    button {
                                        key: "{size}",
                                        style: "padding: 4px 12px; border-radius: 4px; border: 1px solid var(--border); background: {bg}; color: {color}; cursor: pointer; font-size: 11px;",
                                        onclick: move |_| team_size.set(size),
                                        "{size}"
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    style: "display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px;",

                    button {
                        style: "padding: 6px 16px; border-radius: 6px; border: none; background: var(--bgTertiary); color: var(--text); cursor: pointer; font-size: 11px;",
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
                    }
                    button {
                        style: "padding: 6px 16px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px; font-weight: 600;",
                        onclick: move |_| {
                            // TODO: launch swarm via Tauri IPC
                            props.on_close.call(());
                        },
                        "Launch"
                    }
                }
            }
        }
    }
}
