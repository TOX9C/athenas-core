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
use wasm_bindgen::JsCast;

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
    /// Originating pointer id. While a drag is in flight, `pointermove`/`up`
    /// events from any other pointer are ignored — guards multi-touch so a
    /// second finger can't hijack or cancel the drag.
    pub pointer_id: i32,
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
pub(crate) fn walk_to_data_pane_id(el: Option<web_sys::Element>) -> Option<String> {
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

// ---------------------------------------------------------------------------
// Drop-target hit-test — passes through the overlay scrim
// ---------------------------------------------------------------------------

/// Hit-test the drop target at `(x, y)`, transparently passing through the
/// drag overlay scrim. The scrim carries `data-no-drop`; while it's the
/// topmost element under the cursor, we temporarily set
/// `pointer-events:none` on it so `elementFromPoint` sees through to the
/// pane below, then restore it. Returns the target pane id filtered to
/// "not the source" — same-space filtering happens in the store (a swap
/// with a pane id absent from this space no-ops there).
fn find_drop_target(x: f64, y: f64, drag: &PillDrag) -> Option<String> {
    let window = web_sys::window()?;
    let document = window.document()?;
    // Repeatedly find the top element; if it's the scrim, hide it and retry.
    // Cap iterations to avoid infinite loops on unexpected DOM shapes.
    for _ in 0..8 {
        let top = document.element_from_point(x as f32, y as f32);
        match &top {
            Some(el) => {
                if is_overlay_scrim(el) {
                    // Hide the scrim so the next call sees through it.
                    if let Ok(html) = el.clone().dyn_into::<web_sys::HtmlElement>() {
                        let style = html.style();
                        let prev = style.get_property_value("pointer-events").unwrap_or_default();
                        let _ = style.set_property("pointer-events", "none");
                        let result = walk_to_data_pane_id(document.element_from_point(x as f32, y as f32));
                        let _ = style.set_property("pointer-events", &prev);
                        return filter_target(result, drag);
                    }
                    // pointer-events toggle failed — retry once.
                    continue;
                }
                // Hit something that isn't the scrim — walk up from it.
                return filter_target(walk_to_data_pane_id(top), drag);
            }
            None => return None,
        }
    }
    None
}

/// True if `el` is the `PillDragOverlay` scrim (carries a non-empty
/// `data-no-drop` attribute).
fn is_overlay_scrim(el: &web_sys::Element) -> bool {
    el.get_attribute("data-no-drop")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Keep the target only if it's not the source pane. (Same-space filtering is
/// enforced by `swap_pane_agents`, which no-ops if the target id isn't in the
/// source space — so we don't need the target's space id here.)
fn filter_target(found: Option<String>, drag: &PillDrag) -> Option<String> {
    found.and_then(|id| {
        if id == drag.source_pane_id {
            None
        } else {
            Some(id)
        }
    })
}

// ---------------------------------------------------------------------------
// PillDragOverlay — fullscreen pointer-capturing scrim
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct PillDragOverlayProps {
    pub drag: Signal<Option<PillDrag>>,
    pub workspace: Signal<crate::stores::workspace::WorkspaceState>,
    pub terminal_store: Signal<crate::stores::terminal::TerminalStore>,
}

/// Fullscreen transparent overlay that owns `pointermove`/`pointerup` for the
/// duration of a pill drag. Mirrors the proven `DragOverlay` resize pattern
/// in `terminal_grid.rs` (which uses MouseEvent + a fixed overlay) but uses
/// PointerEvents for unified mouse/touch/stylus support. The overlay sits
/// above all content (`z-index: ...dnd-overlay` — see styles.css) so the
/// cursor always lands on it and WKWebView never loses the drag.
#[component]
pub fn PillDragOverlay(props: PillDragOverlayProps) -> Element {
    let mut drag = props.drag;
    let mut workspace = props.workspace;
    let mut terminal_store = props.terminal_store;

    let onpointermove = move |e: PointerEvent| {
        let coords = e.data.client_coordinates();
        let mut current = match drag.read().clone() {
            Some(d) => d,
            None => return,
        };
        // Multi-touch guard: only the originating pointer drives the drag.
        // A second finger pressing mid-drag must not reposition the ghost
        // or change the drop target.
        if e.data.pointer_id() != current.pointer_id {
            return;
        }
        // First move past the threshold commits the drag (lets a pure click
        // — for rename/focus — pass through without starting a swap).
        if !current.moved {
            let dx = coords.x - current.start_x;
            let dy = coords.y - current.start_y;
            if dx * dx + dy * dy < PILL_DRAG_THRESHOLD * PILL_DRAG_THRESHOLD {
                return;
            }
            current.moved = true;
        }
        current.cur_x = coords.x;
        current.cur_y = coords.y;
        current.target_pane_id = find_drop_target(coords.x, coords.y, &current);
        drag.set(Some(current));
    };

    let onpointerup = move |e: PointerEvent| {
        // Multi-touch guard: ignore pointerup from any pointer other than the
        // one that started the drag. (The originating pointer's up commits.)
        let is_origin = drag
            .read()
            .as_ref()
            .map(|d| e.data.pointer_id() == d.pointer_id)
            .unwrap_or(true);
        if !is_origin {
            return;
        }
        let finished = drag.read().clone();
        drag.set(None);
        let Some(d) = finished else {
            return;
        };
        if !d.moved {
            // A click, not a drag — no swap.
            return;
        }
        if let Some(target) = d.target_pane_id.as_ref() {
            if target != &d.source_pane_id {
                {
                    let mut ws = workspace.write();
                    ws.swap_pane_agents(&d.source_space_id, &d.source_pane_id, target);
                }
                // Focus the moved agent at its new slot. `set_active` keys on
                // pane id (not slot index), and `swap_pane_agents` migrated the
                // pane id with the agent, so this stays valid post-swap.
                terminal_store.write().set_active(d.source_pane_id.clone());
            }
        }
        // Dropping outside any pane, or on self → no-op; drag already cleared.
    };

    let is_grabbing = drag
        .read()
        .as_ref()
        .map(|d| d.moved)
        .unwrap_or(false);
    let class = if is_grabbing {
        "dnd-overlay is-grabbing"
    } else {
        "dnd-overlay"
    };

    rsx! {
        div {
            class: "{class}",
            "data-no-drop": "true",
            onpointermove: onpointermove,
            onpointerup: onpointerup,
        }
    }
}

// ---------------------------------------------------------------------------
// PillDragGhost — floating label that follows the cursor
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct PillDragGhostProps {
    pub drag: Signal<Option<PillDrag>>,
}

/// Floating snapshot of the dragged pill's label, fixed under the cursor.
/// Renders nothing while idle or before the threshold is crossed, so it can
/// be mounted unconditionally (no key-flip when a drag starts/ends).
#[component]
pub fn PillDragGhost(props: PillDragGhostProps) -> Element {
    let drag = props.drag;
    let d = match drag.read().clone() {
        Some(d) if d.moved => d,
        _ => return rsx! {},
    };
    rsx! {
        div {
            class: "dnd-ghost",
            style: "left: {d.cur_x:.0}px; top: {d.cur_y:.0}px; border-color: {d.source_color};",
            "{d.source_label}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_to_data_pane_id_none_for_none() {
        assert_eq!(walk_to_data_pane_id(None), None);
    }
}
