use crate::components::shared::icon::IconPlay;
use crate::stores::ui::use_ui_store;
use dioxus::prelude::*;

#[component]
pub fn SwarmLauncher() -> Element {
    let mut ui_state = use_ui_store();

    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: center; padding: 12px;",

            button {
                class: "btn-primary",
                style: "display: inline-flex; align-items: center; gap: 6px;",
                onclick: move |_| {
                    ui_state.write().show_swarm_modal = true;
                },
                IconPlay { size: Some(14), color: Some("currentColor".to_string()) }
                "Launch Swarm"
            }
        }
    }
}
