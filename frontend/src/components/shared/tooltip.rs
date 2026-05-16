use dioxus::prelude::*;

/// Tooltip component - pass-through placeholder.
/// TODO: implement proper tooltip with hover positioning.
#[derive(Props, Clone, PartialEq)]
pub struct TooltipProps {
    pub text: String,
    pub children: Element,
}

#[component]
pub fn Tooltip(props: TooltipProps) -> Element {
    // TODO: implement tooltip with hover positioning
    rsx! {
        div {
            style: "position: relative; display: inline-flex;",
            title: "{props.text}",
            {props.children}
        }
    }
}
