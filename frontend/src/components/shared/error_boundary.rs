use super::illustration::OwlMark;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ErrorBoundaryProps {
    pub children: Element,
    #[props(default = "The oracle is silent.".to_string())]
    pub fallback_message: String,
}

/// Pass-through wrapper (Dioxus lacks React-style catch boundaries). Children render
/// directly; use `ErrorFallback` to render a themed error surface where one is needed.
#[component]
pub fn ErrorBoundary(props: ErrorBoundaryProps) -> Element {
    rsx! { {props.children} }
}

/// A themed error surface — owl mark + message + optional reset action.
#[derive(Props, Clone, PartialEq)]
pub struct ErrorFallbackProps {
    #[props(default = "The oracle is silent.".to_string())]
    pub message: String,
    #[props(default)]
    pub detail: Option<String>,
    #[props(default)]
    pub on_reset: Option<EventHandler<()>>,
}

#[component]
pub fn ErrorFallback(props: ErrorFallbackProps) -> Element {
    rsx! {
        div {
            style: "flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 32px; text-align: center;",
            div { style: "opacity: 0.6;", OwlMark { size: Some(40) } }
            div {
                style: "font-family: var(--font-display); font-size: 20px; font-weight: 600; color: var(--text);",
                "{props.message}"
            }
            if let Some(d) = props.detail {
                div { style: "font-size: var(--text-sm); color: var(--textDim); font-family: var(--fontFamily); max-width: 420px; word-break: break-word;", "{d}" }
            }
            if let Some(reset) = props.on_reset {
                button {
                    class: "btn-secondary btn-sm",
                    onclick: move |_| reset.call(()),
                    "Try again"
                }
            }
        }
    }
}
