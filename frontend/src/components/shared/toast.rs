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
    let toast_state = use_toast_store();

    rsx! {
        div {
            class: "toast-container",
            style: "position: fixed; bottom: 16px; right: 16px; z-index: 100; display: flex; flex-direction: column; gap: 10px; pointer-events: none;",

            for toast in toast_state.read().toasts.iter().cloned() {
                ToastItem { toast }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ToastItemProps {
    pub toast: Toast,
}

#[component]
pub fn ToastItem(props: ToastItemProps) -> Element {
    let toast_id = props.toast.id.clone();
    let local_toast = use_signal(|| props.toast.clone());
    let mut toast_store = use_toast_store();

    let (icon, border_color) = match local_toast.read().toast_type {
        ToastType::Info => ("i", "var(--accent)"),
        ToastType::Success => ("✓", "var(--success)"),
        ToastType::Warning => ("!", "var(--warning)"),
        ToastType::Error => ("X", "var(--error)"),
        ToastType::NeedsInput => ("?", "var(--warning)"),
        ToastType::TaskComplete => ("✓", "var(--success)"),
    };

    rsx! {
        div {
            style: "display: flex; align-items: flex-start; gap: 10px; padding: 12px 16px; border-radius: 8px; background: var(--bgSecondary); color: var(--text); min-width: 280px; max-width: 400px; pointer-events: auto; border: 1px solid var(--border); box-shadow: var(--shadowLg); border-left: 3px solid {border_color};",
            span { style: "font-size: 13px; font-weight: 700; flex-shrink: 0; color: {border_color}; width: 20px; text-align: center;", "{icon}" }
            div {
                style: "flex: 1; min-width: 0;",
                div { style: "font-size: 11px; font-weight: 600; color: var(--text);", "{local_toast.read().title}" }
                div { style: "font-size: 10px; color: var(--textMuted); margin-top: 2px;", "{local_toast.read().message}" }
            }
            button {
                style: "flex-shrink: 0; padding: 2px 6px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 14px; line-height: 1; margin-top: -2px;",
                onclick: move |_| {
                    toast_store.write().remove(&toast_id);
                },
                "×"
            }
        }
    }
}
