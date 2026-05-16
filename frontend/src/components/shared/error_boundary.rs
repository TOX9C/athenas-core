use dioxus::prelude::*;

/// Error boundary component for Dioxus.
/// TODO: implement proper error catching once Dioxus supports error boundaries.
#[derive(Props, Clone, PartialEq)]
pub struct ErrorBoundaryProps {
    pub children: Element,
    #[props(default = "Something went wrong".to_string())]
    pub fallback_message: String,
}

#[component]
pub fn ErrorBoundary(props: ErrorBoundaryProps) -> Element {
    // Dioxus 0.5 does not have React-style error boundaries.
    // We render children directly; a future version may support catching.
    rsx! {
        {props.children}
    }
}
