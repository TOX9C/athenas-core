use dioxus::prelude::*;

// ────────────────shared icon SVG constants──────────────
// Each SVG is inline as a component.  Color is inherited
// from the element scope or the `color` prop, so one icon
// works on light and dark themes.  Size defaults to 16x18
// (viewBox 24x24) but can be overridden via inline style.

/// Render an SVG with the default 24x24 viewBox.
fn inline_svg(children: Element, size: u8, color: &str) -> Element {
    let size_str = format!("{size}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: color,
            stroke_width: "1.5",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {size_str}; height: {size_str}; display: inline-block; vertical-align: middle; overflow: visible;",
            {children}
        }
    }
}

// ──────────────── Action icons ────────────────

#[component]
pub fn IconClose(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    rsx! {
        {inline_svg(
            rsx! {
                line { x1: "18", y1: "6", x2: "6", y2: "18" }
                line { x1: "6", y1: "6", x2: "18", y2: "18" }
            },
            s,
            c,
        )}
    }
}

#[component]
pub fn IconPlus(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            line { x1: "12", y1: "5", x2: "12", y2: "19" }
            line { x1: "5", y1: "12", x2: "19", y2: "12" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconMinus(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            line { x1: "5", y1: "12", x2: "19", y2: "12" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconChevronLeft(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            polyline { points: "15 18 9 12 15 6" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconChevronRight(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            polyline { points: "9 18 15 12 9 6" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconChevronDown(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            polyline { points: "6 9 12 15 18 9" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconChevronUp(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            polyline { points: "18 15 12 9 6 15" }
        },
        s,
        c,
    )
}

// ──────────────── Navigation / Section icons ────────────────

#[component]
pub fn IconSpaces(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "3", y: "3", width: "7", height: "7", rx: "1" }
            rect { x: "14", y: "3", width: "7", height: "7", rx: "1" }
            rect { x: "14", y: "14", width: "7", height: "7", rx: "1" }
            rect { x: "3", y: "14", width: "7", height: "7", rx: "1" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconFiles(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
            polyline { points: "14 2 14 8 20 8" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconAgents(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M16 18a4 4 0 0 0-8 0" }
            circle { cx: "12", cy: "11", r: "3" }
            circle { cx: "20", cy: "11", r: "2" }
            circle { cx: "4", cy: "11", r: "2" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconPlugins(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M20.7 6.5l-3.2-3.2a1 1 0 0 0-1.4 0L3.3 15.1a1 1 0 0 0 0 1.4l3.2 3.2a1 1 0 0 0 1.4 0l12.8-12.8a1 1 0 0 0 0-1.4z" }
            line { x1: "14.1", y1: "6.1", x2: "17.9", y2: "9.9" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconSettings(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "12", cy: "12", r: "3" }
            path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82.33 1.65 1.65 0 0 0-.68 1.58v.14a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.14a1.65 1.65 0 0 0-.68-1.58 1.65 1.65 0 0 0-1.82-.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.58-.68h-.14a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.14a1.65 1.65 0 0 0 1.58-.68 1.65 1.65 0 0 0 .33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33 1.65 1.65 0 0 0 .68-1.58v-.14a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.14a1.65 1.65 0 0 0 .68 1.58 1.65 1.65 0 0 0 1.82.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82z" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconSearch(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "11", cy: "11", r: "8" }
            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconBell(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" }
            path { d: "M13.73 21a2 2 0 0 1-3.46 0" }
        },
        s,
        c,
    )
}

// ──────────────── Feature icons ────────────────

#[component]
pub fn IconTerminal(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            polyline { points: "4 17 10 11 4 5" }
            line { x1: "12", y1: "19", x2: "20", y2: "19" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconZap(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            polygon { points: "13 2 3 14 12 14 11 22 21 10 12 10 13 2" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconSwarm(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "12", cy: "5", r: "2" }
            circle { cx: "5", cy: "19", r: "2" }
            circle { cx: "19", cy: "19", r: "2" }
            line { x1: "11.5", y1: "6.5", x2: "5.5", y2: "18" }
            line { x1: "12.5", y1: "6.5", x2: "18.5", y2: "18" }
            line { x1: "6", y1: "19", x2: "18", y2: "19" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconGrid(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "3", y: "3", width: "7", height: "7", rx: "1" }
            rect { x: "14", y: "3", width: "7", height: "7", rx: "1" }
            rect { x: "14", y: "14", width: "7", height: "7", rx: "1" }
            rect { x: "3", y: "14", width: "7", height: "7", rx: "1" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconFolder(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconMoreHorizontal(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "12", cy: "12", r: "1" }
            circle { cx: "19", cy: "12", r: "1" }
            circle { cx: "5", cy: "12", r: "1" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconEye(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" }
            circle { cx: "12", cy: "12", r: "3" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconEyeOff(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.88 9.88 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" }
            line { x1: "1", y1: "1", x2: "23", y2: "23" }
        },
        s,
        c,
    )
}

// ──────────────── Line-art empty-state illustrations ────────────────
// Minimalist geometric compositions for empty-state screens in the
// style of Linear / Notion / Vercel.

fn empty_svg(children: Element, size: u16, color: &str) -> Element {
    let size_str = format!("{size}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: color,
            stroke_width: "1.4",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {size_str}; height: {size_str}; display: inline-block; overflow: visible;",
            {children}
        }
    }
}

/// Geometric empty workspace -- a wireframe box with an open top.
#[component]
pub fn IconEmptyWorkspace(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            rect { x: "4", y: "5", width: "16", height: "14", rx: "1" }
            line { x1: "4", y1: "12", x2: "20", y2: "12" }
            line { x1: "8", y1: "15", x2: "12", y2: "15" }
        },
        s,
        c,
    )
}

/// Empty chat -- two overlapping speech-bubble outlines.
#[component]
pub fn IconEmptyChat(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M17 8a3 3 0 1 0-4.78 2.82L10 13" }
            path { d: "M13 16a3 3 0 1 0 4.78-2.82L18 10" }
            line { x1: "14", y1: "5", x2: "14", y2: "9" }
            polyline { points: "6.78 3 7 8 2 11 4 6" }
        },
        s,
        c,
    )
}

/// Empty kanban / columns -- three upright rectangles.
#[component]
pub fn IconEmptyKanban(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            rect { x: "3", y: "5", width: "5", height: "14", rx: "1" }
            rect { x: "9", y: "5", width: "6", height: "14", rx: "1" }
            rect { x: "17", y: "5", width: "4", height: "14", rx: "1" }
            rect { x: "4.5", y: "7", width: "2", height: "1" }
            rect { x: "10.5", y: "7", width: "3", height: "1" }
        },
        s,
        c,
    )
}

/// Empty swarm -- a hub-and-spoke network of nodes.
#[component]
pub fn IconEmptySwarm(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            circle { cx: "12", cy: "12", r: "2" }
            line { x1: "12", y1: "10", x2: "12", y2: "5" }
            line { x1: "13.5", y1: "10.5", x2: "17.5", y2: "6.5" }
            line { x1: "13.5", y1: "13.5", x2: "17.5", y2: "17.5" }
            line { x1: "12", y1: "14", x2: "12", y2: "19" }
            line { x1: "10.5", y1: "13.5", x2: "6.5", y2: "17.5" }
            line { x1: "10.5", y1: "10.5", x2: "6.5", y2: "6.5" }
            line { x1: "4", y1: "12", x2: "6.5", y2: "12" }
            line { x1: "17.5", y1: "12", x2: "20", y2: "12" }
            circle { cx: "12", cy: "4", r: "1" }
            circle { cx: "18.5", cy: "6.5", r: "1" }
            circle { cx: "12", cy: "20", r: "1" }
            circle { cx: "5.5", cy: "18", r: "1" }
            circle { cx: "18.5", cy: "18", r: "1" }
            circle { cx: "5.5", cy: "6.5", r: "1" }
            line { x1: "4", y1: "12", x2: "6.5", y2: "12" }
            line { x1: "17.5", y1: "12", x2: "20", y2: "12" }
        },
        s,
        c,
    )
}

/// Empty notifications -- a simple wireframe bell (no motion).
#[component]
pub fn IconEmptyNotifications(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" }
            line { x1: "14", y1: "8", x2: "20", y2: "8" }
        },
        s,
        c,
    )
}

/// Empty plugins -- two interlocking puzzle pieces.
#[component]
pub fn IconEmptyPlugins(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            rect { x: "3", y: "3", width: "9", height: "9", rx: "1" }
            rect { x: "12", y: "12", width: "9", height: "9", rx: "1" }
            line { x1: "17", y1: "12", x2: "17", y2: "3" }
            line { x1: "12", y1: "7", x2: "3", y2: "7" }
        },
        s,
        c,
    )
}

// ──────────────── Glyph-replacement action icons ────────────────
// These exist to retire the text glyphs scattered through the UI
// (×, +, ▶, →, ←, ↻, ☰, ✓, · …).

#[component]
pub fn IconRefresh(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        polyline { points: "23 4 23 10 17 10" }
        polyline { points: "1 20 1 14 7 14" }
        path { d: "M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" }
    }, s, c)
}

#[component]
pub fn IconArrowLeft(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        line { x1: "19", y1: "12", x2: "5", y2: "12" }
        polyline { points: "12 19 5 12 12 5" }
    }, s, c)
}

#[component]
pub fn IconArrowRight(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        line { x1: "5", y1: "12", x2: "19", y2: "12" }
        polyline { points: "12 5 19 12 12 19" }
    }, s, c)
}

#[component]
pub fn IconCheck(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { polyline { points: "20 6 9 17 4 12" } }, s, c)
}

#[component]
pub fn IconCopy(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
        path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
    }, s, c)
}

#[component]
pub fn IconTrash(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        polyline { points: "3 6 5 6 21 6" }
        path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2" }
    }, s, c)
}

#[component]
pub fn IconEdit(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" }
        path { d: "M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" }
    }, s, c)
}

#[component]
pub fn IconSend(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        line { x1: "22", y1: "2", x2: "11", y2: "13" }
        polygon { points: "22 2 15 22 11 13 2 9 22 2" }
    }, s, c)
}

#[component]
pub fn IconMenu(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        line { x1: "3", y1: "6", x2: "21", y2: "6" }
        line { x1: "3", y1: "12", x2: "21", y2: "12" }
        line { x1: "3", y1: "18", x2: "21", y2: "18" }
    }, s, c)
}

#[component]
pub fn IconFile(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" }
        polyline { points: "13 2 13 9 20 9" }
    }, s, c)
}

#[component]
pub fn IconFullscreen(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3m13-5v3a2 2 0 0 1-2 2h-3" }
    }, s, c)
}

#[component]
pub fn IconMinimize(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3" }
    }, s, c)
}

#[component]
pub fn IconGlobe(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        circle { cx: "12", cy: "12", r: "10" }
        line { x1: "2", y1: "12", x2: "22", y2: "12" }
        path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" }
    }, s, c)
}

#[component]
pub fn IconPlay(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { polygon { points: "6 4 20 12 6 20 6 4" } }, s, c)
}

// ──────────────── Mythology motif icons ────────────────
// A small set of classical motifs used for brand, sections, and accents.
// Drawn in the same line style so they read as one family.

/// Owl of Athena — the brand mark / wisdom motif.
#[component]
pub fn IconOwl(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M12 3.4c-4.2 0-7.2 3-7.2 7.2v2.4a7.2 7.2 0 0 0 14.4 0v-2.4c0-4.2-3-7.2-7.2-7.2z" }
        path { d: "M6.7 9.4c-.4-2.4.5-4.1 2-4.4 1.1.8 1.4 2.2 1.1 3.6" }
        path { d: "M17.3 9.4c.4-2.4-.5-4.1-2-4.4-1.1.8-1.4 2.2-1.1 3.6" }
        circle { cx: "9", cy: "10.8", r: "2.4" }
        circle { cx: "15", cy: "10.8", r: "2.4" }
        circle { cx: "9", cy: "10.8", r: "0.7", fill: c }
        circle { cx: "15", cy: "10.8", r: "0.7", fill: c }
        path { d: "M12 12.6l-1.1 1.8h2.2z" }
        path { d: "M9.8 20.7v1.4M12 21.1v1.4M14.2 20.7v1.4" }
    }, s, c)
}

/// Laurel branch — achievement / sections.
#[component]
pub fn IconLaurel(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M12 21V6" }
        path { d: "M12 9c-2 0-3.5-1-4-3 2-.3 3.6.4 4 3z" }
        path { d: "M12 9c2 0 3.5-1 4-3-2-.3-3.6.4-4 3z" }
        path { d: "M12 14c-2.3 0-4-1.2-4.6-3.4 2.3-.4 4.1.5 4.6 3.4z" }
        path { d: "M12 14c2.3 0 4-1.2 4.6-3.4-2.3-.4-4.1.5-4.6 3.4z" }
        path { d: "M12 19c-2.6 0-4.5-1.4-5.2-4 2.6-.5 4.7.6 5.2 4z" }
        path { d: "M12 19c2.6 0 4.5-1.4 5.2-4-2.6-.5-4.7.6-5.2 4z" }
    }, s, c)
}

/// Classical column — structure / workspaces.
#[component]
pub fn IconColumn(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M4 6h16M3 6l2-2h14l2 2M5 6v12M9 6v12M15 6v12M19 6v12M3 20h18M4 18h16l1 2H3z" }
    }, s, c)
}

/// Aegis / shield — protection, security, swarm guard.
#[component]
pub fn IconAegis(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M12 2l8 3v6c0 5-3.5 9-8 11-4.5-2-8-6-8-11V5z" }
        circle { cx: "12", cy: "10", r: "2.2" }
    }, s, c)
}

/// Amphora — storage / sessions.
#[component]
pub fn IconAmphora(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M9 3h6M10 3c0 2-3 3-3 6a5 5 0 0 0 10 0c0-3-3-4-3-6" }
        path { d: "M7 9C5 9 4 10 4 11M17 9c2 0 3 1 3 2" }
        path { d: "M10 19h4l-.5 2h-3z" }
    }, s, c)
}

/// Corinthian helmet — strategy / agents.
#[component]
pub fn IconHelmet(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M5 13a7 7 0 0 1 14 0v3a3 3 0 0 1-3 3H9l-4-2z" }
        path { d: "M12 6V3M12 3c2 0 4 1 5 3" }
        path { d: "M5 13h7v6" }
    }, s, c)
}

/// Scroll — documents / plans.
#[component]
pub fn IconScroll(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M6 4h11a2 2 0 0 1 2 2v11M6 4a2 2 0 0 0-2 2v1h4V6a2 2 0 0 0-2-2z" }
        path { d: "M19 17a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V7h11v10a2 2 0 0 0 2 2z" }
        line { x1: "9", y1: "10", x2: "14", y2: "10" }
        line { x1: "9", y1: "13", x2: "14", y2: "13" }
    }, s, c)
}

/// Greek-key (meander) tile — decorative accent.
#[component]
pub fn IconMeander(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! {
        path { d: "M3 20V8h10v8H7v-4h2" }
        path { d: "M21 4v12H11" }
    }, s, c)
}
