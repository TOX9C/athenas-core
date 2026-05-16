pub mod activity_feed;
pub mod agent_card;
pub mod role_badge;
pub mod swarm_board;
pub mod swarm_launcher;
pub mod swarm_modal;

// Re-export panel
use super::swarm::swarm_board::SwarmBoard;
use super::swarm::swarm_launcher::SwarmLauncher;
use dioxus::prelude::*;

#[component]
pub fn SwarmPanel() -> Element {
    rsx! {
        div {
            class: "swarm-panel",
            style: "height: 100%; display: flex; flex-direction: column;",

            SwarmBoard {}
            SwarmLauncher {}
        }
    }
}
