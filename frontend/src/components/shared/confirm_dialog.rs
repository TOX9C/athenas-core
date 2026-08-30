use super::modal::Modal;
use dioxus::prelude::*;

/// Compact destructive-action confirmation built on the shared [`Modal`].
///
/// Rendered conditionally by the parent: show it when the user requests a
/// destructive action, hide it from either the cancel or confirm handler.
/// Escape, backdrop click, and the X all route to `on_cancel`, so dismissal
/// is always the safe path.
#[derive(Props, Clone, PartialEq)]
pub struct ConfirmDialogProps {
    pub title: String,
    pub message: String,
    /// Label for the destructive action button (defaults to "Delete").
    #[props(default = "Delete".to_string())]
    pub confirm_label: String,
    pub on_cancel: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
}

#[component]
pub fn ConfirmDialog(props: ConfirmDialogProps) -> Element {
    let confirm_label = props.confirm_label.clone();
    rsx! {
        Modal {
            title: props.title.clone(),
            on_close: move |_| props.on_cancel.call(()),
            width: 380,
            compact: true,
            footer: rsx! {
                button {
                    class: "btn-ghost",
                    onclick: move |_| props.on_cancel.call(()),
                    "Cancel"
                }
                button {
                    class: "btn-danger",
                    onclick: move |_| props.on_confirm.call(()),
                    "{confirm_label}"
                }
            },
            div {
                style: "color: var(--textMuted); font-size: var(--text-sm); line-height: 1.5;",
                "{props.message}"
            }
        }
    }
}
