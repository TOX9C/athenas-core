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
            class: "badge",
            style: "display: inline-flex; align-items: center; gap: 4px; font-size: var(--text-2xs); padding: 2px 8px; background: color-mix(in srgb, {color} 14%, transparent); color: {color}; border: 1px solid color-mix(in srgb, {color} 38%, transparent); border-radius: var(--radius-pill); font-weight: 600; letter-spacing: 0.02em; text-transform: capitalize;",
            span {
                style: "width: 5px; height: 5px; border-radius: 50%; background: {color}; flex-shrink: 0;",
            }
            "{label}"
        }
    }
}
