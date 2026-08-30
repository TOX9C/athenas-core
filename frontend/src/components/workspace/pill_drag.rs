//! Pane-pill drag-and-drop: `PillDrag` session state, the fullscreen pointer
//! overlay (`PillDragOverlay`), the floating ghost (`PillDragGhost`), and the
//! `document.elementFromPoint` drop-target hit-test.
//!
//! Mirrors the proven `DragOverlay` resize pattern in `terminal_grid.rs`:
//! a fullscreen fixed overlay mounts on pointerdown and owns pointermove/up,
//! so WKWebView never loses the drag when the cursor leaves the source pill.
//! Uses PointerEvents (not MouseEvent) for unified mouse/touch/stylus.
//!
//! The drag session supports both pane swapping and an Athena context drop
//! target. The fullscreen overlay keeps pointer capture reliable in WKWebView,
//! while DOM hit-testing distinguishes the two destinations.

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

/// Drag threshold in CSS pixels. Below this, a pointerdown is treated as a
/// click (lets double-click rename + icon-button clicks pass through).
pub const PILL_DRAG_THRESHOLD: f64 = 4.0;

/// Where a pane-pill drag can be released.
#[derive(Clone, Debug, PartialEq)]
pub enum PillDropTarget {
    /// Swap the dragged pane with another pane in the same workspace.
    Pane(String),
    /// Pin the dragged agent as context in Athena.
    Athena,
}

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
    /// Hit-tested release destination, or `None` when outside a valid target.
    pub target: Option<PillDropTarget>,
    /// Pane type copied into Athena context when this drag is released there.
    pub source_agent_type: String,
    /// Whether this pane can be referenced by Athena. Agent and plain shell
    /// panes are both valid sources; the type is retained for the context chip
    /// and prompt metadata.
    pub source_can_reference: bool,
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

/// Resolve the nearest supported drop destination while walking up from a
/// hit-tested element. Pane IDs take precedence because pane wrappers are
/// nested inside the workspace, while Athena is a sibling surface.
fn walk_to_drop_target(el: Option<web_sys::Element>) -> Option<PillDropTarget> {
    let mut node = el;
    while let Some(elem) = node {
        if let Some(value) = elem.get_attribute("data-pane-id") {
            return Some(PillDropTarget::Pane(value));
        }
        if elem
            .get_attribute("data-athena-drop")
            .is_some_and(|value| !value.is_empty())
        {
            return Some(PillDropTarget::Athena);
        }
        node = elem.parent_element();
    }
    None
}

/// Find the topmost pane under screen point `(x, y)` by calling
/// `document.elementFromPoint` then walking up to the nearest
/// `[data-pane-id]` ancestor. Returns its value, or `None` if the point is
/// not over any pane (e.g. over the sidebar, the titlebar, or the drag-overlay
/// scrim itself). This legacy helper remains useful to callers that only need
/// pane resolution; the full drag path uses `find_drop_target`.
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
fn find_drop_target(x: f64, y: f64, drag: &PillDrag) -> Option<PillDropTarget> {
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
                        let prev = style
                            .get_property_value("pointer-events")
                            .unwrap_or_default();
                        let _ = style.set_property("pointer-events", "none");
                        let result =
                            walk_to_drop_target(document.element_from_point(x as f32, y as f32));
                        let _ = style.set_property("pointer-events", &prev);
                        let result = filter_target(result, drag);
                        set_athena_drag_hover(matches!(result, Some(PillDropTarget::Athena)));
                        return result;
                    }
                    // pointer-events toggle failed — retry once.
                    continue;
                }
                // Hit something that isn't the scrim — walk up from it.
                let result = filter_target(walk_to_drop_target(top), drag);
                set_athena_drag_hover(matches!(result, Some(PillDropTarget::Athena)));
                return result;
            }
            None => {
                set_athena_drag_hover(false);
                return None;
            }
        }
    }
    set_athena_drag_hover(false);
    None
}

/// Toggle the visual state on Athena without introducing a second global
/// Dioxus signal solely for a pointer-move paint effect. The drag overlay is
/// above the panel, so Athena itself cannot receive `:hover` while dragging.
fn set_athena_drag_hover(active: bool) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Ok(nodes) = document.query_selector_all("[data-athena-drop]") else {
        return;
    };
    for index in 0..nodes.length() {
        if let Some(node) = nodes.item(index) {
            if let Ok(element) = node.dyn_into::<web_sys::Element>() {
                let mut classes = element
                    .get_attribute("class")
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                let has_class = classes.iter().any(|class| class == "is-dnd-target");
                if active && !has_class {
                    classes.push("is-dnd-target".to_string());
                } else if !active {
                    classes.retain(|class| class != "is-dnd-target");
                }
                let _ = element.set_attribute("class", &classes.join(" "));
            }
        }
    }
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
fn filter_target(found: Option<PillDropTarget>, drag: &PillDrag) -> Option<PillDropTarget> {
    found.and_then(|target| match target {
        PillDropTarget::Pane(id) if id == drag.source_pane_id => None,
        other => Some(other),
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
    pub athena_store: Signal<crate::stores::athena::AthenaState>,
    pub panel_store: Signal<crate::stores::panel_manager::PanelManagerState>,
    pub ui_store: Signal<crate::stores::ui::UIState>,
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
    let mut athena_store = props.athena_store;
    let mut panel_store = props.panel_store;
    let mut ui_store = props.ui_store;

    // Window-level cancellation matters when WKWebView backgrounds the app or
    // the pointer leaves the document before the overlay receives pointerup.
    // Keep the closures alive until unmount and remove both listeners in the
    // same hook lifetime.
    let cancel_handler: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::Event)>>>> =
        use_hook(|| Rc::new(RefCell::new(None)));
    let cancel_handler_for_effect = cancel_handler.clone();
    use_effect(move || {
        if cancel_handler_for_effect.borrow().is_some() {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let mut drag_for_window = drag;
        let handler = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            drag_for_window.set(None);
            set_athena_drag_hover(false);
        }) as Box<dyn FnMut(web_sys::Event)>);
        let callback = handler.as_ref().unchecked_ref();
        let _ = window.add_event_listener_with_callback("blur", callback);
        let _ = document.add_event_listener_with_callback("visibilitychange", callback);
        *cancel_handler_for_effect.borrow_mut() = Some(handler);
    });
    let cancel_handler_for_drop = cancel_handler.clone();
    use_drop(move || {
        if let (Some(window), Some(handler)) = (
            web_sys::window(),
            cancel_handler_for_drop.borrow_mut().take(),
        ) {
            let callback = handler.as_ref().unchecked_ref();
            let _ = window.remove_event_listener_with_callback("blur", callback);
            if let Some(document) = window.document() {
                let _ = document.remove_event_listener_with_callback("visibilitychange", callback);
            }
        }
    });

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
        current.target = find_drop_target(coords.x, coords.y, &current);
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
        let mut finished = drag.read().clone();
        let Some(current) = finished.as_mut() else {
            return;
        };
        if current.moved {
            // Re-hit-test at release time. The last pointermove can be stale
            // when the pointer crosses the target between browser frames.
            let coords = e.data.client_coordinates();
            current.cur_x = coords.x;
            current.cur_y = coords.y;
            let target = find_drop_target(coords.x, coords.y, current);
            current.target = target;
        }
        drag.set(None);
        set_athena_drag_hover(false);
        let Some(d) = finished else {
            return;
        };
        if !d.moved {
            // A click, not a drag — no swap.
            return;
        }
        match d.target {
            Some(PillDropTarget::Pane(target)) if target != d.source_pane_id => {
                let mut ws = workspace.write();
                ws.swap_pane_agents(&d.source_space_id, &d.source_pane_id, &target);
                // Focus the moved agent at its new slot. `set_active` keys on
                // pane id (not slot index), and `swap_pane_agents` migrated the
                // pane id with the agent, so this stays valid post-swap.
                terminal_store.write().set_active(d.source_pane_id);
            }
            Some(PillDropTarget::Athena) if d.source_can_reference => {
                let added = athena_store.write().add_agent_context(
                    d.source_pane_id,
                    d.source_agent_type,
                    d.source_label,
                );
                // Always reveal Athena after a valid drop. If the reference was
                // already present, opening the panel still gives the user a
                // deterministic acknowledgement instead of a silent no-op.
                panel_store
                    .write()
                    .open_right_panel(crate::stores::panel_manager::RightPanel::Assistant);
                ui_store.write().right_sidebar_open = true;
                web_sys::console::log_1(
                    &format!(
                        "[athena-dnd] reference {}",
                        if added { "added" } else { "already present" }
                    )
                    .into(),
                );
            }
            _ => {}
        }
        // Dropping outside any valid target, or on self, is a no-op.
    };

    let onpointercancel = move |e: PointerEvent| {
        let is_origin = drag
            .read()
            .as_ref()
            .map(|current| e.data.pointer_id() == current.pointer_id)
            .unwrap_or(true);
        if !is_origin {
            return;
        }
        drag.set(None);
        set_athena_drag_hover(false);
    };
    let is_grabbing = drag.read().as_ref().map(|d| d.moved).unwrap_or(false);
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
            onpointercancel: onpointercancel,
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
            // Per-frame: only the cursor position changes. `source_color` is
            // constant for the whole drag, so it goes on a CSS custom property
            // (read by `.dnd-ghost` for the border color) rather than restated
            // as a per-frame `border-color` declaration. Keeping the dynamic
            // string to two integers avoids a per-frame `format!` allocation
            // of the full style in the ~60fps pointermove hot path.
            style: "--dnd-ghost-color: {d.source_color}; left: {d.cur_x:.0}px; top: {d.cur_y:.0}px;",
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

    fn test_drag() -> PillDrag {
        PillDrag {
            source_pane_id: "pane-1".into(),
            source_space_id: "space-1".into(),
            source_label: "Builder".into(),
            source_color: "var(--accent)".into(),
            pointer_id: 1,
            start_x: 0.0,
            start_y: 0.0,
            cur_x: 0.0,
            cur_y: 0.0,
            moved: true,
            target: None,
            source_agent_type: "claude".into(),
            source_can_reference: true,
        }
    }

    #[test]
    fn filter_target_rejects_source_but_keeps_other_panes_and_athena() {
        let drag = test_drag();
        assert_eq!(
            filter_target(Some(PillDropTarget::Pane("pane-1".into())), &drag),
            None
        );
        assert_eq!(
            filter_target(Some(PillDropTarget::Pane("pane-2".into())), &drag),
            Some(PillDropTarget::Pane("pane-2".into()))
        );
        assert_eq!(
            filter_target(Some(PillDropTarget::Athena), &drag),
            Some(PillDropTarget::Athena)
        );
    }
}
