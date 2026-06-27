use crate::components::shared::toast::{use_toast_store, ToastItem};
use dioxus::prelude::*;

#[component]
pub fn NotificationToast() -> Element {
    let toasts = use_toast_store();

    rsx! {
        div {
            class: "notification-toast-container",
            style: "position: fixed; bottom: 18px; right: 18px; z-index: 100; display: flex; flex-direction: column; gap: 12px; pointer-events: none;",

            for toast in toasts.read().toasts.iter() {
                ToastItem { key: "{toast.id}", toast: toast.clone() }
            }
        }
    }
}
