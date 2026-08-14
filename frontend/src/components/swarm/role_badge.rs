use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SwarmRoleBadgeProps {
    pub role: String,
}

#[component]
pub fn SwarmRoleBadge(props: SwarmRoleBadgeProps) -> Element {
    let (color, label) = match props.role.to_lowercase().as_str() {
        "coordinator" => ("#0ea5e9", "Coordinator"),
        "builder" => ("#22c55e", "Builder"),
        "scout" => ("#f59e0b", "Scout"),
        "reviewer" => ("#06b6d4", "Reviewer"),
        _ => ("var(--textDim)", props.role.as_str()),
    };

    rsx! {
        span {
            class: "status-label",
            style: "color: {color}; text-transform: capitalize;",
            "{label}"
        }
    }
}
