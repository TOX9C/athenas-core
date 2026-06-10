use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::stores::terminal::{use_terminal_store, TerminalCell, TerminalColor};
use crate::stores::workspace::{use_workspace_store, AgentType, Space};
use crate::tauri_bridge::pty_kill;
use crate::utils::agent_commands::{get_agent_color, get_agent_label};
use crate::components::shared::icon::{IconClose, IconFullscreen};
use crate::components::shared::illustration::{EmptyState, EmptyArt};

#[cfg(feature = "xterm")]
use crate::components::workspace::xterm_mount::XtermMount;

#[cfg(feature = "xterm")]
fn render_shell_pane(
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    resume_id: Option<String>,
    custom_cmd: Option<String>,
) -> Element {
    rsx! { XtermMount { key: "xterm-{pane_id}", pane_id, cwd, agent_type, resume_id, custom_cmd } }
}

#[cfg(not(feature = "xterm"))]
fn render_shell_pane(pane_id: String, _cwd: String) -> Element {
    rsx! { TerminalPaneBody { key: "terminal-body-{pane_id}", pane_id } }
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

/// Props for the workspace grid.
#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceGridProps {
    pub active_space: Option<Space>,
    pub active_space_id: Option<String>,
}

// ---------------------------------------------------------------------------
// DragInfo & DragKind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct DragInfo {
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
enum DragKind {
    Col,
    Row,
}

// ---------------------------------------------------------------------------
// WorkspaceGrid
// ---------------------------------------------------------------------------

#[component]
pub fn WorkspaceGrid(props: WorkspaceGridProps) -> Element {
    let space = match props.active_space {
        Some(s) => s,
        None => return rsx! { div {} },
    };

    let pane_count = space.panes.len();
    let (cols, _rows) = match space.grid {
        crate::types::workspace::GridTemplate::X1x1 => (1usize, 1usize),
        crate::types::workspace::GridTemplate::X1x2 => (2, 1),
        crate::types::workspace::GridTemplate::X2x2 => (2, 2),
        crate::types::workspace::GridTemplate::X2x3 => (3, 2),
        crate::types::workspace::GridTemplate::X3x3 => (3, 3),
        crate::types::workspace::GridTemplate::X3x4 => (4, 3),
        crate::types::workspace::GridTemplate::X4x4 => (4, 4),
    };

    if pane_count == 0 {
        return rsx! {
            EmptyState {
                kind: EmptyArt::Workspace,
                title: "Empty workspace".to_string(),
                hint: Some("Add a shell or agent to begin.".to_string()),
            }
        };
    }

    let actual_row_count = (pane_count + cols - 1) / cols;

    // Per-row column flex-grow values. Each row keeps its own width state, so
    // resizing the top row does not force the bottom row to match it.
    let mut col_widths = use_signal(|| {
        (0..actual_row_count)
            .map(|row_idx| {
                let start = row_idx * cols;
                let row_len = (pane_count.saturating_sub(start)).min(cols).max(1);
                vec![1.0_f64; row_len]
            })
            .collect::<Vec<_>>()
    });

    // Row heights remain row-scoped across the whole grid.
    let mut row_heights = use_signal(|| vec![1.0_f64; actual_row_count.max(1)]);
    let drag = use_signal(|| None::<DragInfo>);
    let terminal_store = use_terminal_store();
    let active_pane_id = terminal_store.read().active_session_id.clone();
    // Note: active pane selection is stored in TerminalStore (single source of truth).
    // The clicked pane gets a subtle gold focus ring (see `.pane-focus-ring`).

    use_effect(move || {
        let target_width_shape: Vec<usize> = (0..actual_row_count)
            .map(|row_idx| {
                let start = row_idx * cols;
                (pane_count.saturating_sub(start)).min(cols).max(1)
            })
            .collect();
        let current_width_shape: Vec<usize> =
            col_widths.peek().iter().map(|row| row.len()).collect();
        if current_width_shape != target_width_shape {
            col_widths.set(
                target_width_shape
                    .into_iter()
                    .map(|len| vec![1.0_f64; len])
                    .collect(),
            );
        }

        let target_row_count = actual_row_count.max(1);
        if row_heights.peek().len() != target_row_count {
            row_heights.set(vec![1.0_f64; target_row_count]);
        }
    });

    rsx! {
        div {
            class: "workspace-grid-root",
            "data-space-id": "{space.id}",
            style: "flex: 1; display: flex; flex-direction: column; gap: 0; padding: 0; overflow: hidden; background: var(--bg); min-height: 0; min-width: 0; position: relative;",

            for row_idx in 0..actual_row_count {
                {
                    let start_pane = row_idx * cols;
                    let end_pane = ((row_idx + 1) * cols).min(pane_count);
                    let row_panes = &space.panes[start_pane..end_pane];
                    let row_weight = row_heights.read().get(row_idx).copied().unwrap_or(1.0);

                    rsx! {
                        div {
                            key: "row-{space.id}-{row_idx}",
                            style: "display: flex; flex-direction: row; flex: {row_weight}; min-height: 0; min-width: 0; gap: 0;",

                            for (rel_idx, pane) in row_panes.iter().enumerate() {
                                {
                                    let flex_weight = {
                                        let cw = col_widths.read();
                                        if let Some(row) = cw.get(row_idx) {
                                            *row.get(rel_idx).unwrap_or(&1.0)
                                        } else {
                                            1.0
                                        }
                                    };
                                    let has_right = rel_idx + 1 < row_panes.len();
                                    let has_bottom = row_idx + 1 < actual_row_count;
                                    let is_active = active_pane_id.as_deref() == Some(pane.id.as_str());

                                    let mut wrapper_style = format!(
                                        "position: relative; flex: {}; min-height: 0; min-width: 0; padding: 0; display: flex; flex-direction: column; box-sizing: border-box;",
                                        flex_weight
                                    );
                                    if has_right {
                                        wrapper_style.push_str(" border-right: 1px solid color-mix(in srgb, var(--border, #888) 58%, transparent);");
                                    }
                                    if has_bottom {
                                        wrapper_style.push_str(" border-bottom: 1px solid color-mix(in srgb, var(--border, #888) 58%, transparent);");
                                    }

                                    rsx! {
                                        div {
                                            key: "pane-wrap-{space.id}-{pane.id}",
                                            style: "{wrapper_style}",

                                            if is_active {
                                                div { class: "pane-focus-ring" }
                                            }

                                            PaneItem {
                                                key: "pane-{space.id}-{pane.id}",
                                                space_id: space.id.clone(),
                                                pane_id: pane.id.clone(),
                                                cwd: space.dir.clone(),
                                                agent_type: pane.agent_type.clone(),
                                                is_shell: matches!(pane.agent_type, AgentType::Shell | AgentType::Custom),
                                                resume_id: pane.resume_id.clone(),
                                                custom_cmd: pane.custom_cmd.clone(),
                                            }
                                        }

                                        if rel_idx + 1 < row_panes.len() {
                                            ColDivider {
                                                key: "col-div-{row_idx}-{rel_idx}",
                                                space_id: space.id.clone(),
                                                row_index: row_idx,
                                                index: rel_idx,
                                                col_widths: col_widths,
                                                drag: drag,
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if row_idx + 1 < actual_row_count {
                            RowDivider {
                                key: "row-div-{row_idx}",
                                space_id: space.id.clone(),
                                index: row_idx,
                                row_heights: row_heights,
                                drag: drag,
                            }
                        }
                    }
                }
            }
        }

        if drag.cloned().is_some() {
            DragOverlay {
                drag: drag,
                col_widths: col_widths,
                row_heights: row_heights,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PaneItem
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct PaneItemProps {
    space_id: String,
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    is_shell: bool,
    resume_id: Option<String>,
    custom_cmd: Option<String>,
}

#[component]
fn PaneItem(props: PaneItemProps) -> Element {
    let mut workspace = use_workspace_store();
    let mut terminal_store = use_terminal_store();

    let pane_id_for_close = props.pane_id.clone();
    let space_id_for_close = props.space_id.clone();
    let agent_label = get_agent_label(&props.agent_type);
    let _agent_color = get_agent_color(&props.agent_type);
    let _display_id: String = props.pane_id.chars().take(10).collect();

    rsx! {
        div {
            style: "flex: 1; width: 100%; height: 100%; min-width: 0; min-height: 0; display: flex; flex-direction: column; background: var(--bg); overflow: hidden; box-sizing: border-box;",
            onpointerdown: move |_| {
                terminal_store.write().set_active(props.pane_id.clone());
            },

            // Pill header — distinct, refined, sits inside the pane
            div {
                style: "flex-shrink: 0; padding: 6px 8px 0 8px;",

                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 4px 12px; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: 999px; flex-shrink: 0;",

                    span {
                        style: "display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 16px; color: var(--accent); padding: 0;",
                        dangerous_inner_html: "<svg viewBox='0 0 24 24' fill='currentColor' width='14' height='14'><circle cx='12' cy='12' r='3'/><circle cx='12' cy='3' r='3'/><circle cx='12' cy='21' r='3'/></svg>",
                    }
                    span {
                        style: "font-family: var(--font-ui); font-size: var(--text-xs); font-weight: 600; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{agent_label}"
                    }
                    div {
                        style: "display: flex; align-items: center; gap: 4px; margin-left: auto;",
                        button {
                            class: "icon-btn",
                            title: "Fullscreen",
                            IconFullscreen { size: Some(12), color: Some("currentColor".to_string()) }
                        }

                        button {
                            class: "icon-btn",
                            title: "Close pane",
                            onclick: move |e| {
                                e.stop_propagation();
                                {
                                    let mut ws = workspace.write();
                                    ws.remove_pane_from_space(&space_id_for_close, &pane_id_for_close);
                                }
                                {
                                    let mut term = terminal_store.write();
                                    term.sessions.remove(&pane_id_for_close);
                                    if term.active_session_id.as_deref() == Some(&pane_id_for_close) {
                                        term.active_session_id = term.sessions.keys().next().cloned();
                                    }
                                    term.generation = term.generation.wrapping_add(1);
                                }
                                spawn({
                                    let pane_id = pane_id_for_close.clone();
                                    async move {
                                        let _ = pty_kill(&pane_id).await;
                                    }
                                });
                            },
                            IconClose { size: Some(14), color: Some("currentColor".to_string()) }
                        }
                    }
                }
            }

            // Shell body — flat, no padding, fills edge-to-edge below the pill header
            div {
                style: "flex: 1; min-width: 0; min-height: 0; padding: 0; background: var(--bg); overflow: hidden;",
                if props.is_shell {
                    { render_shell_pane(props.pane_id.clone(), props.cwd.clone(), props.agent_type.clone(), props.resume_id.clone(), props.custom_cmd.clone()) }
                } else {
                    TerminalPaneBody { pane_id: props.pane_id.clone() }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalPaneBody
// ---------------------------------------------------------------------------

#[component]
fn TerminalPaneBody(pane_id: String) -> Element {
    let store = use_terminal_store();
    let Bud = {
        let s = store.read();
        s.sessions
            .get(&pane_id)
            .map(|session| session.grid.clone())
            .unwrap_or_default()
    };

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; background: var(--bg); overflow: hidden; padding: 0;",
            div {
                style: "font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace; font-size: 11px; line-height: 1.4; color: var(--text); white-space: pre-wrap; overflow-wrap: break-word;",
                if Bud.is_empty() {
                    "Waiting for output..."
                } else {
                    for (row_idx, row) in Bud.iter().enumerate() {
                        div {
                            key: "row-{row_idx}",
                            style: "display: flex;",
                            for cell in row.iter() {
                                TerminalCellItem { cell: cell.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalCellItem
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct TerminalCellItemProps {
    cell: TerminalCell,
}

#[component]
fn TerminalCellItem(props: TerminalCellItemProps) -> Element {
    let cell = &props.cell;
    let fg = color_to_css(&cell.fg);
    let bg = color_to_css(&cell.bg);
    let bold = if cell.bold { "font-weight: bold;" } else { "" };
    let style = format!("color: {}; background-color: {}; {}", fg, bg, bold);

    rsx! {
        span {
            style: "{style}",
            "{cell.text}"
        }
    }
}

// ---------------------------------------------------------------------------
// ColDivider
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct ColDividerProps {
    space_id: String,
    row_index: usize,
    index: usize,
    col_widths: Signal<Vec<Vec<f64>>>,
    drag: Signal<Option<DragInfo>>,
}

#[component]
fn ColDivider(props: ColDividerProps) -> Element {
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
            index: index,
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
struct RowDividerProps {
    space_id: String,
    index: usize,
    row_heights: Signal<Vec<f64>>,
    drag: Signal<Option<DragInfo>>,
}

#[component]
fn RowDivider(props: RowDividerProps) -> Element {
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
            index: index,
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
struct DragOverlayProps {
    drag: Signal<Option<DragInfo>>,
    col_widths: Signal<Vec<Vec<f64>>>,
    row_heights: Signal<Vec<f64>>,
}

#[component]
fn DragOverlay(props: DragOverlayProps) -> Element {
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
    let actual_rows = if cols == 0 {
        0
    } else {
        (pane_count + cols - 1) / cols
    };
    actual_rows.saturating_sub(1)
}

/// Convert a TerminalColor to a CSS color string.
fn color_to_css(color: &TerminalColor) -> String {
    match color {
        TerminalColor::Default => "inherit".to_string(),
        TerminalColor::Black => "#000000".to_string(),
        TerminalColor::Red => "#ef4444".to_string(),
        TerminalColor::Green => "#22c55e".to_string(),
        TerminalColor::Yellow => "#eab308".to_string(),
        TerminalColor::Blue => "#3b82f6".to_string(),
        TerminalColor::Magenta => "#a855f7".to_string(),
        TerminalColor::Cyan => "#06b6d4".to_string(),
        TerminalColor::White => "#ffffff".to_string(),
        TerminalColor::BrightBlack => "#374151".to_string(),
        TerminalColor::BrightRed => "#f87171".to_string(),
        TerminalColor::BrightGreen => "#4ade80".to_string(),
        TerminalColor::BrightYellow => "#facc15".to_string(),
        TerminalColor::BrightBlue => "#60a5fa".to_string(),
        TerminalColor::BrightMagenta => "#c084fc".to_string(),
        TerminalColor::BrightCyan => "#22d3ee".to_string(),
        TerminalColor::BrightWhite => "#f9fafb".to_string(),
        TerminalColor::Indexed(idx) => ansi256_to_rgb(*idx),
        TerminalColor::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
    }
}

/// Convert an ANSI 256 color index to an RGB hex string.
fn ansi256_to_rgb(idx: u8) -> String {
    // Standard 16 colors
    if idx < 16 {
        let colors = [
            "#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0",
            "#808080", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff",
        ];
        return colors[idx as usize].to_string();
    }
    // 216 color cube (16-231)
    if idx < 232 {
        let cube_idx = (idx as usize) - 16;
        let r = (cube_idx / 36) * 51;
        let g = ((cube_idx % 36) / 6) * 51;
        let b = (cube_idx % 6) * 51;
        return format!("#{:02x}{:02x}{:02x}", r, g, b);
    }
    // Grayscale (232-255)
    let gray = 8 + ((idx as usize) - 232) * 10;
    format!("#{:02x}{:02x}{:02x}", gray, gray, gray)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
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

    #[test]
    fn test_color_to_css_default() {
        assert_eq!(color_to_css(&TerminalColor::Default), "inherit");
    }

    #[test]
    fn test_ansi256_to_rgb() {
        assert_eq!(ansi256_to_rgb(0), "#000000");
        assert_eq!(ansi256_to_rgb(1), "#800000");
        assert_eq!(ansi256_to_rgb(16), "#000000");
        assert_eq!(ansi256_to_rgb(232), "#080808");
        assert_eq!(ansi256_to_rgb(255), "#eeeeee");
    }
}
