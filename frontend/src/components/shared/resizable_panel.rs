use dioxus::prelude::*;

/// Simple resizable panel wrapper.
/// TODO: implement drag-to-resize with divider.
#[derive(Props, Clone, PartialEq)]
pub struct ResizablePanelProps {
    pub children: Element,
}

#[component]
pub fn ResizablePanel(props: ResizablePanelProps) -> Element {
    rsx! {
        div {
            style: "flex: 1; min-width: 0; min-height: 0; overflow: hidden;",
            {props.children}
        }
    }
}
