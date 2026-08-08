//! Native terminal-grid cell rendering used when the xterm renderer is disabled.

use crate::stores::terminal::{use_terminal_registry, TerminalCell, TerminalColor};
use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// TerminalPaneBody
// ---------------------------------------------------------------------------

#[component]
pub(crate) fn TerminalPaneBody(pane_id: String) -> Element {
    // Subscribe to THIS pane's inner signal for grid snapshots (Item 3). A
    // cell delta in pane A re-clones only pane A's grid here; pane B's memo
    // doesn't re-evaluate. `use_memo` caches the clone until the signal moves.
    //
    // `use_terminal_registry()` is a hook — captured once, synchronously, at
    // render top; `registry.session_signal(...)` is a plain method safe to
    // call inside the `use_memo` closure. Calling `use_session_signal(...)`
    // here would re-enter the hook list (it warps `use_context`) and panic
    // Dioxus at mount with "hook list already borrowed".
    let terminal_registry = use_terminal_registry();
    let Bud = use_memo(move || {
        terminal_registry
            .session_signal(&pane_id)
            .and_then(|s| s.try_read().ok().map(|r| r.grid.clone()))
            .unwrap_or_default()
    })();

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
