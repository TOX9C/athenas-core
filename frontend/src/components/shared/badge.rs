use dioxus::prelude::*;

/// Small status/label pill. `color` is the foreground/accent; the background is
/// derived as a translucent tint of it via color-mix, so a single hue reads as a
/// cohesive badge across themes.
#[derive(Props, Clone, PartialEq)]
pub struct BadgeProps {
    pub label: String,
    #[props(default = "var(--accent)".to_string())]
    pub color: String,
    /// Optional explicit text color. Defaults to `color`.
    #[props(default)]
    pub text_color: Option<String>,
    /// Solid fill instead of tinted (e.g. count badges).
    #[props(default = false)]
    pub solid: bool,
}

#[component]
pub fn Badge(props: BadgeProps) -> Element {
    let fg = props
        .text_color
        .clone()
        .unwrap_or_else(|| props.color.clone());
    let style = if props.solid {
        format!(
            "background: {c}; color: var(--bg);",
            c = props.color
        )
    } else {
        format!(
            "background: color-mix(in srgb, {c} 15%, transparent); color: {fg}; border: 1px solid color-mix(in srgb, {c} 28%, transparent);",
            c = props.color
        )
    };
    rsx! {
        span {
            style: "display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border-radius: var(--radius-pill); font-size: var(--text-2xs); font-weight: 600; letter-spacing: 0.02em; line-height: 1.6; {style}",
            "{props.label}"
        }
    }
}
