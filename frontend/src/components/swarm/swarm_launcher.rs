use crate::components::shared::icon::IconPlay;
use dioxus::prelude::*;

#[component]
pub fn SwarmLauncher() -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: center; padding: 12px;",

            button {
                class: "btn-primary",
                style: "display: inline-flex; align-items: center; gap: 6px;",
                onclick: move |_| {
                    // TODO: open SwarmModal
                },
                IconPlay { size: Some(14), color: Some("currentColor".to_string()) }
                "Launch Swarm"
            }
        }
    }
}
