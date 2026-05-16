use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct BadgeProps {
    pub label: String,
    #[props(default = "var(--accent)".to_string())]
    pub color: String,
    #[props(default = "var(--text)".to_string())]
    pub text_color: String,
}

#[component]
pub fn Badge(props: BadgeProps) -> Element {
    rsx! {
        span {
            style: "display: inline-flex; align-items: center; padding: 1px 6px; border-radius: 9999px; font-size: 9px; font-weight: 600; background: {props.color}; color: {props.text_color};",
            "{props.label}"
        }
    }
}
