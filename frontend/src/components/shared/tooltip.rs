use dioxus::prelude::*;

/// Themed hover tooltip. Replaces the native `title` attribute with a positioned,
/// theme-styled bubble. Defaults to appearing above the trigger.
#[derive(Props, Clone, PartialEq)]
pub struct TooltipProps {
    pub text: String,
    /// "top" (default), "bottom", "left", "right".
    #[props(default = "top".to_string())]
    pub placement: String,
    pub children: Element,
}

#[component]
pub fn Tooltip(props: TooltipProps) -> Element {
    let mut show = use_signal(|| false);

    let pos = match props.placement.as_str() {
        "bottom" => "top: calc(100% + 6px); left: 50%; transform: translateX(-50%);",
        "left" => "right: calc(100% + 6px); top: 50%; transform: translateY(-50%);",
        "right" => "left: calc(100% + 6px); top: 50%; transform: translateY(-50%);",
        _ => "bottom: calc(100% + 6px); left: 50%; transform: translateX(-50%);",
    };

    rsx! {
        div {
            style: "position: relative; display: inline-flex;",
            onmouseenter: move |_| show.set(true),
            onmouseleave: move |_| show.set(false),
            {props.children}
            if show() {
                div { class: "tip-bubble", style: "{pos}", "{props.text}" }
            }
        }
    }
}
