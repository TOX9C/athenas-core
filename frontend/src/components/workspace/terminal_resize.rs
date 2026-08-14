//! Resizable workspace-grid dividers and pointer tracking.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

// ---------------------------------------------------------------------------
// DragInfo & DragKind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(super) struct DragInfo {
    kind: DragKind,
    scope_index: Option<usize>,
    index: usize,
    start_x: f64,
    start_y: f64,
    initial_left: f64,
    initial_right: f64,
    dimension_pixels: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DragKind {
    Col,
    Row,
}

// ---------------------------------------------------------------------------
// ColDivider
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub(super) struct ColDividerProps {
    space_id: String,
    row_index: usize,
    index: usize,
    col_widths: Signal<Vec<Vec<f64>>>,
    drag: Signal<Option<DragInfo>>,
}

#[component]
pub(super) fn ColDivider(props: ColDividerProps) -> Element {
    let mut drag = props.drag;
    let col_widths = props.col_widths;
    let row_index = props.row_index;
    let index = props.index;

    let space_id_for_col_resize = props.space_id.clone();

    let onmousedown = move |e: MouseEvent| {
        let coords = e.data.client_coordinates();
        let (initial_left, initial_right) = {
            let widths = col_widths.read();
            let row = widths.get(row_index);
            let left = row.and_then(|r| r.get(index)).copied().unwrap_or(1.0);
            let right = row.and_then(|r| r.get(index + 1)).copied().unwrap_or(1.0);
            (left, right)
        };
        let dimension_pixels =
            workspace_grid_dimension(DragKind::Col, &space_id_for_col_resize).unwrap_or(0.0);
        drag.set(Some(DragInfo {
            kind: DragKind::Col,
            scope_index: Some(row_index),
            index,
            start_x: coords.x,
            start_y: coords.y,
            initial_left,
            initial_right,
            dimension_pixels,
        }));
    };

    let is_dragging = matches!(
        &*drag.read(),
        Some(d) if d.kind == DragKind::Col && d.scope_index == Some(row_index) && d.index == index
    );

    rsx! {
        div {
            style: "position: relative; width: 0; min-width: 0; flex-shrink: 0; overflow: visible; z-index: 2;",
            div {
                class: if is_dragging { "pane-divider-col is-dragging" } else { "pane-divider-col" },
                onmousedown: onmousedown,
                title: "Resize panes",
                style: "position: absolute; left: -4px; top: 0; bottom: 0; width: 8px; cursor: col-resize;",
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RowDivider
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub(super) struct RowDividerProps {
    space_id: String,
    index: usize,
    row_heights: Signal<Vec<f64>>,
    drag: Signal<Option<DragInfo>>,
}

#[component]
pub(super) fn RowDivider(props: RowDividerProps) -> Element {
    let mut drag = props.drag;
    let row_heights = props.row_heights;
    let index = props.index;

    let space_id_for_row_resize = props.space_id.clone();

    let onmousedown = move |e: MouseEvent| {
        let coords = e.data.client_coordinates();
        let (initial_left, initial_right) = {
            let heights = row_heights.read();
            let left = heights.get(index).copied().unwrap_or(1.0);
            let right = heights.get(index + 1).copied().unwrap_or(1.0);
            (left, right)
        };
        let dimension_pixels =
            workspace_grid_dimension(DragKind::Row, &space_id_for_row_resize).unwrap_or(0.0);
        drag.set(Some(DragInfo {
            kind: DragKind::Row,
            scope_index: None,
            index,
            start_x: coords.x,
            start_y: coords.y,
            initial_left,
            initial_right,
            dimension_pixels,
        }));
    };

    let is_dragging = matches!(
        &*drag.read(),
        Some(d) if d.kind == DragKind::Row && d.index == index
    );

    rsx! {
        div {
            style: "position: relative; height: 0; min-height: 0; flex-shrink: 0; overflow: visible; z-index: 2;",
            div {
                class: if is_dragging { "pane-divider-row is-dragging" } else { "pane-divider-row" },
                onmousedown: onmousedown,
                title: "Resize panes",
                style: "position: absolute; top: -4px; left: 0; right: 0; height: 8px; cursor: row-resize;",
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DragOverlay
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub(super) struct DragOverlayProps {
    drag: Signal<Option<DragInfo>>,
    col_widths: Signal<Vec<Vec<f64>>>,
    row_heights: Signal<Vec<f64>>,
}

#[component]
pub(super) fn DragOverlay(props: DragOverlayProps) -> Element {
    let mut drag = props.drag;
    let mut col_widths = props.col_widths;
    let mut row_heights = props.row_heights;

    let onmousemove = move |e: MouseEvent| {
        if let Some(drag_info) = *drag.read() {
            let coords = e.data.client_coordinates();
            let total = drag_info.initial_left + drag_info.initial_right;
            let dimension = drag_info.dimension_pixels;
            if dimension <= 0.0 {
                return;
            }

            match drag_info.kind {
                DragKind::Col => {
                    if let Some(row_idx) = drag_info.scope_index {
                        let delta = coords.x - drag_info.start_x;
                        let mut cw = col_widths.write();
                        if let Some(row) = cw.get_mut(row_idx) {
                            if let Some((left, right)) = resize_pair_from_drag(
                                drag_info.initial_left,
                                total,
                                delta,
                                dimension,
                            ) {
                                row[drag_info.index] = left;
                                row[drag_info.index + 1] = right;
                            }
                        }
                    }
                }
                DragKind::Row => {
                    let delta = coords.y - drag_info.start_y;
                    let mut rh = row_heights.write();
                    let len = rh.len();
                    if len > 1 && drag_info.index < len - 1 {
                        if let Some((left, right)) =
                            resize_pair_from_drag(drag_info.initial_left, total, delta, dimension)
                        {
                            rh[drag_info.index] = left;
                            rh[drag_info.index + 1] = right;
                        }
                    }
                }
            }
        }
    };

    let onmouseup = move |_e: MouseEvent| {
        drag.set(None);
    };

    let cursor = match *drag.read() {
        Some(DragInfo {
            kind: DragKind::Col,
            ..
        }) => "col-resize",
        Some(DragInfo {
            kind: DragKind::Row,
            ..
        }) => "row-resize",
        None => "default",
    };

    rsx! {
        div {
            style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; z-index: 9999; cursor: {cursor}; background: transparent;",
            onmousemove: onmousemove,
            onmouseup: onmouseup,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Resize a pair of columns/rows preserving total weight using the drag
/// start state and the current cursor delta relative to the grid dimension.
fn resize_pair_from_drag(
    initial_left: f64,
    total: f64,
    delta_pixels: f64,
    total_pixels: f64,
) -> Option<(f64, f64)> {
    if total_pixels <= 0.0 || total <= 0.0 {
        return None;
    }
    let delta_ratio = delta_pixels / total_pixels;
    let new_left = (initial_left + delta_ratio * total)
        .max(0.1)
        .min(total - 0.1);
    let new_right = total - new_left;
    Some((new_left, new_right))
}

fn workspace_grid_dimension(kind: DragKind, space_id: &str) -> Option<f64> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let selector = format!(".workspace-grid-root[data-space-id=\"{}\"]", space_id);
    let element = document.query_selector(&selector).ok()??;
    let html_el = element.dyn_into::<web_sys::HtmlElement>().ok()?;
    let rect = html_el.get_bounding_client_rect();
    match kind {
        DragKind::Col => Some(rect.width()),
        DragKind::Row => Some(rect.height()),
    }
}

/// Calculate the number of horizontal row dividers between rendered rows.
#[cfg(test)]
fn row_divider_count(pane_count: usize, cols: usize, _rows: usize) -> usize {
    let actual_rows = std::num::NonZeroUsize::new(cols)
        .map(|cols| pane_count.div_ceil(cols.get()))
        .unwrap_or(0);
    actual_rows.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize_pair_from_drag() {
        let result = resize_pair_from_drag(1.0, 2.0, 50.0, 200.0);
        assert!(result.is_some());
        let (left, right) = result.unwrap();
        assert!(left > 1.0);
        assert!(right < 1.0);
        assert!((left + right - 2.0).abs() < f64::EPSILON);

        let result = resize_pair_from_drag(1.0, 2.0, -1000.0, 200.0).unwrap();
        assert_eq!(result, (0.1, 1.9));
    }

    #[test]
    fn test_row_divider_count() {
        assert_eq!(row_divider_count(0, 2, 2), 0);
        assert_eq!(row_divider_count(1, 2, 2), 0);
        assert_eq!(row_divider_count(2, 2, 2), 0);
        assert_eq!(row_divider_count(3, 2, 2), 1);
        assert_eq!(row_divider_count(4, 2, 2), 1);
    }
}
