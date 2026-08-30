use crate::utils::agent_display::{get_role_color_str, get_role_label_str};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SwarmRoleBadgeProps {
    pub role: String,
}

#[component]
pub fn SwarmRoleBadge(props: SwarmRoleBadgeProps) -> Element {
    let role_key = props.role.to_lowercase();
    let color = get_role_color_str(&role_key);
    let label = get_role_label_str(&role_key);

    rsx! {
        span {
            class: "status-label",
            style: "color: {color}; text-transform: capitalize;",
            "{label}"
        }
    }
}
