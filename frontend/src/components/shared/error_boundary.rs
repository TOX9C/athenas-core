use super::illustration::CoreMark;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ErrorBoundaryProps {
    pub children: Element,
    #[props(default = "Something went wrong.".to_string())]
    pub fallback_message: String,
}

/// Recoverable Dioxus error boundary for a subtree.
///
/// This handles errors reported through Dioxus's error context. It does not
/// revive a WebAssembly instance after an unrecoverable `unreachable` trap;
/// the JavaScript watchdog in `frontend/index.html` is responsible for that
/// outer recovery path.
#[component]
pub fn ErrorBoundary(props: ErrorBoundaryProps) -> Element {
    let fallback_message = props.fallback_message.clone();
    rsx! {
        dioxus::prelude::ErrorBoundary {
            handle_error: move |_errors: ErrorContext| {
                let message = fallback_message.clone();
                rsx! { ErrorFallback { message } }
            },
            {props.children}
        }
    }
}

/// A themed error surface — core mark + message + optional reset action.
#[derive(Props, Clone, PartialEq)]
pub struct ErrorFallbackProps {
    #[props(default = "Something went wrong.".to_string())]
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
            div { style: "opacity: 0.6;", CoreMark { size: Some(40) } }
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
