// TODO: This is a placeholder from a React port. Dioxus doesn't have
// React-style error boundaries. This component should be replaced
// with a proper error handling strategy (e.g., error-propagating contexts
// or removed entirely).

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ErrorBoundaryProps {
    pub children: Element,
    #[props(default = "Something went wrong".to_string())]
    pub fallback_message: String,
}

#[component]
pub fn ErrorBoundary(props: ErrorBoundaryProps) -> Element {
    // TODO: Replace with proper error handling once Dioxus supports it.
    rsx! { {props.children} }
}
