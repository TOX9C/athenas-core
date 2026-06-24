use dioxus::prelude::*;

/// Flex wrapper for a resizable region's content (parent owns the width state).
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

/// A themed drag handle (1px hairline, no hover glow). The parent
/// supplies the mousedown handler and owns the drag math + global capture overlay.
#[derive(Props, Clone, PartialEq)]
pub struct ResizeHandleProps {
    pub on_mouse_down: EventHandler<MouseEvent>,
    /// true = horizontal divider (row resize); false = vertical divider (col resize).
    #[props(default = false)]
    pub vertical: bool,
    #[props(default = false)]
    pub dragging: bool,
}

#[component]
pub fn ResizeHandle(props: ResizeHandleProps) -> Element {
    let base = if props.vertical {
        "height: 1px; width: 100%; cursor: row-resize;"
    } else {
        "width: 1px; height: 100%; cursor: col-resize;"
    };
    let cls = if props.dragging {
        "resize-handle is-dragging"
    } else {
        "resize-handle"
    };
    let hit = if props.vertical {
        "position: absolute; left: 0; right: 0; top: -4px; height: 9px;"
    } else {
        "position: absolute; top: 0; bottom: 0; left: -4px; width: 9px;"
    };
    rsx! {
        div {
            class: "{cls}",
            style: "position: relative; flex-shrink: 0; align-self: stretch; {base}",
            onmousedown: move |e: MouseEvent| { e.prevent_default(); props.on_mouse_down.call(e); },
            div { style: "{hit} background: transparent;" }
        }
    }
}
