//! Pane-pill drag-and-drop: `PillDrag` session state, the fullscreen pointer
//! overlay (`PillDragOverlay`), the floating ghost (`PillDragGhost`), and the
//! `document.elementFromPoint` drop-target hit-test.
//!
//! Mirrors the proven `DragOverlay` resize pattern in `terminal_grid.rs`:
//! a fullscreen fixed overlay mounts on pointerdown and owns pointermove/up,
//! so WKWebView never loses the drag when the cursor leaves the source pill.
//! Uses PointerEvents (not MouseEvent) for unified mouse/touch/stylus.
//!
//! Task 2 scope: `PillDrag` state struct, `PILL_DRAG_THRESHOLD`, the pure
//! `walk_to_data_pane_id` DOM-walk helper, and the `nearest_pane_id_under_point`
//! JS-interop hit-test. The `PillDragOverlay`/`PillDragGhost` components are
//! added in Task 3.

use dioxus::prelude::*;
use web_sys::Element;

/// Drag threshold in CSS pixels. Below this, a pointerdown is treated as a
/// click (lets double-click rename + icon-button clicks pass through).
pub const PILL_DRAG_THRESHOLD: f64 = 4.0;

/// A live pane-pill drag session. `None` (on the owning `Signal`) when idle.
#[derive(Clone, Debug)]
pub struct PillDrag {
    /// Pane id being dragged (the source pill's pane).
    pub source_pane_id: String,
    /// Space the drag started in. Drops only swap within the same space.
    pub source_space_id: String,
    /// Pill text shown on the ghost.
    pub source_label: String,
    /// Agent color shown on the ghost (CSS color string).
    pub source_color: String,
    /// pointer-down position (client coords).
    pub start_x: f64,
    /// pointer-down position (client coords).
    pub start_y: f64,
    /// Latest pointer position (client coords), updated on pointermove.
    pub cur_x: f64,
    /// Latest pointer position (client coords), updated on pointermove.
    pub cur_y: f64,
    /// Has the pointer crossed the threshold? If false, pointerup is a click
    /// (no swap, no drag preview shown).
    pub moved: bool,
    /// Hit-tested target pane id (same space, not the source), or `None`.
    pub target_pane_id: Option<String>,
}

/// Walk up the DOM from `el` to the nearest ancestor (inclusive) carrying a
/// `data-pane-id` attribute, returning its value. Pure function over the DOM
/// — extracted so the walk logic is unit-testable in isolation.
///
/// `data-pane-id` is set on pane-wrapper `div`s by `terminal_grid.rs` (Task 4).
pub(crate) fn walk_to_data_pane_id(el: Option<Element>) -> Option<String> {
    let mut node = el;
    while let Some(elem) = node {
        if let Some(value) = elem.get_attribute("data-pane-id") {
            return Some(value);
        }
        node = elem.parent_element();
    }
    None
}

/// Find the topmost pane under screen point `(x, y)` by calling
/// `document.elementFromPoint` then walking up to the nearest
/// `[data-pane-id]` ancestor. Returns its value, or `None` if the point is
/// not over any pane (e.g. over the sidebar, the titlebar, or the drag-overlay
/// scrim itself).
///
/// Note: `document.elementFromPoint` returns the topmost element at the point
/// — during a drag this is usually the `PillDragOverlay` scrim (it has
/// `pointer-events: auto` to receive events). `PillDragOverlay` (Task 3)
/// temporarily hides the scrim from hit-testing via `find_drop_target`; this
/// pure helper does only the DOM walk.
pub fn nearest_pane_id_under_point(x: f64, y: f64) -> Option<String> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let element = document.element_from_point(x as f32, y as f32);
    walk_to_data_pane_id(element)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_to_data_pane_id_none_for_none() {
        assert_eq!(walk_to_data_pane_id(None), None);
    }
}
