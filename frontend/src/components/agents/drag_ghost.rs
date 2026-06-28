use dioxus::prelude::*;

use crate::components::agents::drag_layer::{DragLayer, DragPayload};

/// Floating ghost element that follows the cursor during drag operations.
///
/// Render this at the app root level (e.g., in lib.rs or App component)
/// so it floats above all other content.
#[component]
pub fn DragGhost(layer: Signal<DragLayer>) -> Element {
    let active = layer.read().active.read().clone();

    let text = match active {
        Some(DragPayload::GridPane { pane_label, .. }) => pane_label,
        Some(DragPayload::Agent { label, .. }) => label,
        None => return rsx! { div {} },
    };

    rsx! {
        div {
            class: "drag-ghost",
            "{text}"
        }
    }
}
