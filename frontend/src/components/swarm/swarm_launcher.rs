use dioxus::prelude::*;

#[component]
pub fn SwarmLauncher() -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: center; padding: 12px;",

            button {
                style: "padding: 8px 16px; border-radius: 8px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 12px; font-weight: 600;",
                onclick: move |_| {
                    // TODO: open SwarmModal
                },
                "Launch Swarm"
            }
        }
    }
}
