use dioxus::prelude::*;

/// Toast notification type.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ToastType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
    NeedsInput,
    TaskComplete,
}

/// A single toast notification.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Toast {
    pub id: String,
    pub toast_type: ToastType,
    pub title: String,
    pub message: String,
    pub duration_ms: u64,
}

/// Global toast state.
#[derive(Clone, PartialEq, Default)]
pub struct ToastState {
    pub toasts: Vec<Toast>,
}

impl ToastState {
    pub fn push(&mut self, toast: Toast) {
        self.toasts.push(toast);
    }

    pub fn remove(&mut self, id: &str) {
        self.toasts.retain(|t| t.id != id);
    }
}

pub fn use_toast_store() -> Signal<ToastState> {
    // TODO: wire to proper global context
    use_context::<Signal<ToastState>>()
}

pub fn provide_toast_store() {
    use_context_provider(|| Signal::new(ToastState::default()));
}

/// Show a notification toast programmatically.
pub fn show_notification_toast(
    _toast_type: ToastType,
    _title: &str,
    _message: &str,
    _agent_type: Option<&str>,
) {
    // TODO: wire to Tauri IPC or global signal for toast dispatch
}

#[component]
pub fn ToastContainer() -> Element {
    // TODO: wire to toast signal; for now render nothing
    rsx! {
        div {
            class: "toast-container",
            style: "position: fixed; bottom: 16px; right: 16px; z-index: 100; display: flex; flex-direction: column; gap: 8px; pointer-events: none;",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ToastItemProps {
    pub toast: Toast,
}

#[component]
pub fn ToastItem(props: ToastItemProps) -> Element {
    let (icon, bg, color) = match props.toast.toast_type {
        ToastType::Info => ("\u{2139}", "var(--accent)", "#fff"),
        ToastType::Success => ("\u{2713}", "var(--success)", "#fff"),
        ToastType::Warning => ("\u{26a0}", "var(--warning)", "#fff"),
        ToastType::Error => ("\u{2717}", "var(--error)", "#fff"),
        ToastType::NeedsInput => ("\u{2753}", "var(--warning)", "#fff"),
        ToastType::TaskComplete => ("\u{2705}", "var(--success)", "#fff"),
    };

    rsx! {
        div {
            style: "display: flex; align-items: flex-start; gap: 8px; padding: 10px 14px; border-radius: 8px; background: {bg}; color: {color}; min-width: 280px; max-width: 400px; pointer-events: auto; box-shadow: 0 8px 24px rgba(0,0,0,0.3);",
            span { style: "font-size: 14px; flex-shrink: 0;", "{icon}" }
            div {
                style: "flex: 1; min-width: 0;",
                div { style: "font-size: 11px; font-weight: 600;", "{props.toast.title}" }
                div { style: "font-size: 10px; opacity: 0.85; margin-top: 2px;", "{props.toast.message}" }
            }
        }
    }
}
