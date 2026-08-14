use dioxus::prelude::*;

/// Small status/label marker. The neutral surface keeps labels legible without
/// turning every piece of metadata into a colored pill.
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
            "background: {c}; color: var(--bg); border: 1px solid {c};",
            c = props.color
        )
    } else {
        format!(
            "background: var(--bgTertiary); color: {fg}; border: 1px solid var(--border);",
            fg = fg
        )
    };
    rsx! {
        span {
            style: "display: inline-flex; align-items: center; gap: 4px; padding: 2px 6px; border-radius: var(--radius-sm); font-size: var(--text-2xs); font-weight: 500; letter-spacing: 0.03em; line-height: 1.4; {style}",
            "{props.label}"
        }
    }
}
