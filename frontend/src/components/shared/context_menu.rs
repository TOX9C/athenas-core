use dioxus::prelude::*;

/// Context menu component - pass-through placeholder.
/// TODO: implement right-click context menu with positioning.
#[derive(Props, Clone, PartialEq)]
pub struct ContextMenuProps {
    pub children: Element,
}

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    rsx! {
        div {
            style: "display: contents;",
            {props.children}
        }
    }
}
