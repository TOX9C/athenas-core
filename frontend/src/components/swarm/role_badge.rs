use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SwarmRoleBadgeProps {
    pub role: String,
}

#[component]
pub fn SwarmRoleBadge(props: SwarmRoleBadgeProps) -> Element {
    let (color, label) = match props.role.as_str() {
        "coordinator" => ("#0ea5e9", "Coordinator"),
        "builder" => ("#22c55e", "Builder"),
        "scout" => ("#f59e0b", "Scout"),
        "reviewer" => ("#06b6d4", "Reviewer"),
        other => ("var(--textDim)", other),
    };

    rsx! {
        span {
            style: "font-size: 8px; padding: 1px 5px; border-radius: 3px; background: {color}22; color: {color}; font-weight: 600;",
            "{label}"
        }
    }
}
