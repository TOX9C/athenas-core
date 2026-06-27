use dioxus::prelude::*;

use crate::components::agents::drag_layer::{DragLayer, DragPayload};

/// Floating ghost element that follows the cursor during drag operations.
///
/// Render this at the app root level (e.g., in lib.rs or App component)
/// so it floats above all other content.
#[component]
pub fn DragGhost(layer: Signal<DragLayer>) -> Element {
    let active = layer.read().active.read().clone();
    let (x, y) = *layer.read().cursor_xy.read();

    let text = match active {
        Some(DragPayload::GridPane { pane_label, .. }) => pane_label,
        Some(DragPayload::Agent { label, .. }) => label,
        None => return rsx! { div {} },
    };

    rsx! {
        div {
            class: "drag-ghost",
            style: "position: fixed; pointer-events: none; z-index: 9999; top: {y}px; left: {x}px;",
            div {
                style: "padding: 6px 12px; background: var(--accent); color: var(--bg); border-radius: 999px; font-size: 12px; font-weight: 600; white-space: nowrap; box-shadow: 0 4px 12px rgba(0,0,0,0.3); opacity: 0.85;",
                "{text}"
            }
        }
    }
}
