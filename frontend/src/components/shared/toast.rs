use super::icon::{IconBell, IconCheck, IconClose, IconOwl};
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

    let ttype = local_toast.read().toast_type.clone();

    // Auto-remove toast after duration_ms. Cancelled on unmount via use_drop
    // to avoid stale writes to the store if the component is removed early.
    let duration = local_toast.read().duration_ms;
    let dismiss_id = toast_id.clone();
    let mut dismiss = use_future(move || {
        let id = dismiss_id.clone();
        let mut store = toast_store;
        async move {
            if duration > 0 {
                gloo::timers::future::TimeoutFuture::new(duration as u32).await;
            } else {
                gloo::timers::future::TimeoutFuture::new(1).await;
            }
            // Idempotent: retain() is a no-op if id is already gone.
            store.write().remove(&id);
        }
    });
    use_drop(move || {
        dismiss.cancel();
    });

    let color = match ttype {
        ToastType::Info => "var(--accent)",
        ToastType::Success | ToastType::TaskComplete => "var(--success)",
        ToastType::Warning | ToastType::NeedsInput => "var(--warning)",
        ToastType::Error => "var(--error)",
    };
    let icon = match ttype {
        ToastType::Success | ToastType::TaskComplete => {
            rsx! { IconCheck { size: Some(13), color: Some(color.to_string()) } }
        }
        ToastType::Error => rsx! { IconClose { size: Some(13), color: Some(color.to_string()) } },
        ToastType::Warning | ToastType::NeedsInput => {
            rsx! { IconBell { size: Some(13), color: Some(color.to_string()) } }
        }
        ToastType::Info => rsx! { IconOwl { size: Some(13), color: Some(color.to_string()) } },
    };

    rsx! {
        div {
            class: "toast-card",
            style: "display: flex; align-items: flex-start; gap: 11px; padding: 12px 14px; background: var(--bgSecondary); color: var(--text); min-width: 280px; max-width: 400px; pointer-events: auto; border: 1px solid var(--border); border-left: 3px solid {color}; border-radius: var(--radius-md);",
            span {
                style: "flex-shrink: 0; width: 22px; height: 22px; display: flex; align-items: center; justify-content: center; border-radius: var(--radius-pill); background: color-mix(in srgb, {color} 16%, transparent);",
                {icon}
            }
            div {
                style: "flex: 1; min-width: 0;",
                div { style: "font-size: var(--text-sm); font-weight: 600; color: var(--text);", "{local_toast.read().title}" }
                div { style: "font-size: var(--text-xs); color: var(--textMuted); margin-top: 2px; line-height: 1.45;", "{local_toast.read().message}" }
            }
            button {
                class: "icon-btn",
                style: "width: 22px; height: 22px; flex-shrink: 0;",
                "aria-label": "Dismiss",
                onclick: move |_| { toast_store.write().remove(&toast_id); },
                IconClose { size: Some(13), color: Some("currentColor".to_string()) }
            }
        }
    }
}
