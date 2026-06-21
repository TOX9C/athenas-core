use dioxus::prelude::*;

// ──────────────── shared icon SVG constants ──────────────
// Greek-mythology icon family. Each motif is drawn with smooth
// Bézier paths on a 24×24 grid, single 1.5px stroke, round caps.
// Color is inherited from the element scope or the `color` prop,
// so one icon works on light and dark themes.

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

// ──────────────── Action icons (smooth geometry, universal) ────────────────

#[component]
pub fn IconClose(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M6.5 6.5 L17.5 17.5" }
            path { d: "M17.5 6.5 L6.5 17.5" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconPlus(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M12 5 V19" }
            path { d: "M5 12 H19" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconMinus(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M5 12 H19" } }, s, c)
}

#[component]
pub fn IconChevronLeft(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M15 6 L9 12 L15 18" } }, s, c)
}

#[component]
pub fn IconChevronRight(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M9 6 L15 12 L9 18" } }, s, c)
}

#[component]
pub fn IconChevronDown(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M6 9 L12 15 L18 9" } }, s, c)
}

#[component]
pub fn IconChevronUp(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M18 15 L12 9 L6 15" } }, s, c)
}

// ──────────────── Navigation / Section icons (mythology motifs) ────────────────

/// Spaces — 2×2 meander-key tiles.
#[component]
pub fn IconSpaces(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // four tiles
            rect { x: "3", y: "3", width: "7.5", height: "7.5", rx: "1.2" }
            rect { x: "13.5", y: "3", width: "7.5", height: "7.5", rx: "1.2" }
            rect { x: "13.5", y: "13.5", width: "7.5", height: "7.5", rx: "1.2" }
            rect { x: "3", y: "13.5", width: "7.5", height: "7.5", rx: "1.2" }
            // meander key etched into each tile
            path { d: "M5 7 h3 v-2" }
            path { d: "M15.5 7 h3 v-2" }
            path { d: "M15.5 17 h3 v-2" }
            path { d: "M5 17 h3 v-2" }
        },
        s,
        c,
    )
}

/// Files — stacked papyrus scrolls.
#[component]
pub fn IconFiles(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // back scroll (rolled rod at right)
            path { d: "M5 7.5 C5 6 6 5 7.5 5 H14 C15.5 5 16.5 6 16.5 7.5 V16.5 C16.5 18 15.5 19 14 19 H7.5 C6 19 5 18 5 16.5 Z" }
            // front scroll rod
            path { d: "M16.5 7.5 C18 7.5 19 8.5 19 10 V17 C19 18 18 19 16.5 19" }
            // text lines
            path { d: "M8 10 H13" }
            path { d: "M8 13 H13" }
            path { d: "M8 16 H11" }
        },
        s,
        c,
    )
}

/// Agents — Corinthian helmet.
#[component]
pub fn IconAgents(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // dome
            path { d: "M6 14 V12 C6 8 8.5 5 12 5 C15.5 5 18 8 18 12 V14" }
            // cheek guards curling inward
            path { d: "M6 14 C6 16.5 7.5 18 9.5 18 V13" }
            path { d: "M18 14 C18 16.5 16.5 18 14.5 18 V13" }
            // nose guard (the T of the Corinthian)
            path { d: "M12 9 V17" }
            // crest plume
            path { d: "M8 6 C9 3.5 15 3.5 16 6" }
        },
        s,
        c,
    )
}

/// Plugins — puzzle piece (modularity). Legible over motif.
#[component]
pub fn IconPlugins(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M4 5 H14 V10 C16 10 16 14 14 14 V19 H4 Z" }
            path { d: "M14 19 V17 H17 V14 C18.8 14 18.8 10 17 10" }
        },
        s,
        c,
    )
}

/// Settings — gear (universal affordance).
#[component]
pub fn IconSettings(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "12", cy: "12", r: "3" }
            path { d: "M12 2 V5 M12 19 V22 M2 12 H5 M19 12 H22 M4.9 4.9 L7 7 M17 17 L19.1 19.1 M19.1 4.9 L17 7 M7 17 L4.9 19.1" }
        },
        s,
        c,
    )
}

/// Search — smooth lens (affordance kept).
#[component]
pub fn IconSearch(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "11", cy: "11", r: "7" }
            path { d: "M16.2 16.2 L21 21" }
        },
        s,
        c,
    )
}

/// Bell — notification bell (universal affordance).
#[component]
pub fn IconBell(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M18 8 A6 6 0 0 0 6 8 C6 15 3 17 3 17 H21 C21 17 18 15 18 8 Z" }
            path { d: "M13.7 21 A2 2 0 0 1 10.3 21" }
        },
        s,
        c,
    )
}

// ──────────────── Feature icons ────────────────

/// Terminal — smooth `>` prompt (affordance kept).
#[component]
pub fn IconTerminal(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M5 7 L10 12 L5 17" }
            path { d: "M13 17 H19" }
        },
        s,
        c,
    )
}

/// Zap — Keraunos, Zeus's thunderbolt (forked).
#[component]
pub fn IconZap(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M13 2 L5 13 H11 L9 22 L19 9 H13 L15 2 Z" }
        },
        s,
        c,
    )
}

/// Swarm — constellation of connected stars.
#[component]
pub fn IconSwarm(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // connecting lines
            path { d: "M12 6 L5.5 18 L18.5 18 Z" }
            path { d: "M12 6 V18" }
            // star nodes
            path { d: "M12 4 L12.8 5.6 L14.5 5.8 L13.2 7 L13.6 8.7 L12 7.9 L10.4 8.7 L10.8 7 L9.5 5.8 L11.2 5.6 Z" }
            circle { cx: "5.5", cy: "18", r: "1.4", fill: c }
            circle { cx: "18.5", cy: "18", r: "1.4", fill: c }
        },
        s,
        c,
    )
}

/// Grid — meander-key 2×2 tiles (alias of Spaces geometry).
#[component]
pub fn IconGrid(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "3", y: "3", width: "7.5", height: "7.5", rx: "1.2" }
            rect { x: "13.5", y: "3", width: "7.5", height: "7.5", rx: "1.2" }
            rect { x: "13.5", y: "13.5", width: "7.5", height: "7.5", rx: "1.2" }
            rect { x: "3", y: "13.5", width: "7.5", height: "7.5", rx: "1.2" }
        },
        s,
        c,
    )
}

/// Folder — universal folder silhouette.
#[component]
pub fn IconFolder(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M3 7 A2 2 0 0 1 5 5 H9 L11 7 H19 A2 2 0 0 1 21 9 V18 A2 2 0 0 1 19 20 H5 A2 2 0 0 1 3 18 Z" }
        },
        s,
        c,
    )
}

/// More — three olive dots.
#[component]
pub fn IconMoreHorizontal(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "5", cy: "12", r: "1.3", fill: c }
            circle { cx: "12", cy: "12", r: "1.3", fill: c }
            circle { cx: "19", cy: "12", r: "1.3", fill: c }
        },
        s,
        c,
    )
}

/// Eye — view (universal affordance).
#[component]
pub fn IconEye(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M2 12 C5 7 8 5 12 5 C16 5 19 7 22 12 C19 17 16 19 12 19 C8 19 5 17 2 12 Z" }
            circle { cx: "12", cy: "12", r: "3" }
        },
        s,
        c,
    )
}

/// EyeOff — hidden (universal affordance).
#[component]
pub fn IconEyeOff(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M10.6 5.1 C11 5 11.5 5 12 5 C16 5 19 7 22 12 C21.1 13.5 20.1 14.7 19 15.6" }
            path { d: "M6.6 6.6 C4.8 7.8 3.3 9.6 2 12 C5 17 8 19 12 19 C13.2 19 14.3 18.8 15.3 18.4" }
            path { d: "M9.9 9.9 A3 3 0 0 0 14 14" }
            path { d: "M3 3 L21 21" }
        },
        s,
        c,
    )
}

// ──────────────── Line-art empty-state illustrations ────────────────
// Minimalist mythology compositions for empty-state screens.
// Drawn in the same smooth line style as the icon family.

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

/// Geometric empty workspace — open amphora.
#[component]
pub fn IconEmptyWorkspace(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M9 5 H15" }
            path { d: "M10 5 C10 7 7.5 8 7.5 11 C7.5 12.5 8 14 8 15 A4 4 0 0 0 16 15 C16 14 16.5 12.5 16.5 11 C16.5 8 14 7 14 5" }
            path { d: "M7.5 11 C6 11 5 12 5 13" }
            path { d: "M16.5 11 C18 11 19 12 19 13" }
            path { d: "M10 19 H14 L13.5 21 H10.5 Z" }
        },
        s,
        c,
    )
}

/// Empty chat — two overlapping speech scrolls.
#[component]
pub fn IconEmptyChat(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M5 7 C5 6 6 5 7 5 H14 C15 5 16 6 16 7 V13 C16 14 15 15 14 15 H10 L7 18 V15 H7 C6 15 5 14 5 13 Z" }
            path { d: "M11 11 C11 10 12 9 13 9 H18 C19 9 20 10 20 11 V16 C20 17 19 18 18 18 H15 V20 L13 18 H13 C12 18 11 17 11 16 Z" }
        },
        s,
        c,
    )
}

/// Empty kanban — three temple columns.
#[component]
pub fn IconEmptyKanban(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            // pediment
            path { d: "M3 7 L12 4 L21 7" }
            path { d: "M3 7 H21" }
            // columns
            path { d: "M6 7 V18" }
            path { d: "M12 7 V18" }
            path { d: "M18 7 V18" }
            // stylobate base
            path { d: "M3 18 H21" }
            path { d: "M2 21 H22" }
        },
        s,
        c,
    )
}

/// Empty swarm — constellation hub-and-spoke.
#[component]
pub fn IconEmptySwarm(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M12 10 V5 M13.5 10.5 L17.5 6.5 M13.5 13.5 L17.5 17.5 M12 14 V19 M10.5 13.5 L6.5 17.5 M10.5 10.5 L6.5 6.5" }
            circle { cx: "12", cy: "12", r: "2.2" }
            path { d: "M12 3 L12.7 4.5 L14.3 4.6 L13.1 5.6 L13.5 7.2 L12 6.4 L10.5 7.2 L10.9 5.6 L9.7 4.6 L11.3 4.5 Z" }
            circle { cx: "18.5", cy: "6.5", r: "1.1" }
            circle { cx: "18.5", cy: "18", r: "1.1" }
            circle { cx: "12", cy: "20.5", r: "1.1" }
            circle { cx: "5.5", cy: "18", r: "1.1" }
            circle { cx: "5.5", cy: "6.5", r: "1.1" }
        },
        s,
        c,
    )
}

/// Empty notifications — sleeping owl (no motion).
#[component]
pub fn IconEmptyNotifications(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M5 9 C5 6 8 4 12 4 C16 4 19 6 19 9 V13 C19 16 16 18 12 18 C8 18 5 16 5 13 Z" }
            path { d: "M8 9 C8.5 7.5 9.5 7 10.5 7.2" }
            path { d: "M16 9 C15.5 7.5 14.5 7 13.5 7.2" }
            path { d: "M9 11 C10 12.5 14 12.5 15 11" }
            path { d: "M11 14 L13 14" }
        },
        s,
        c,
    )
}

/// Empty plugins — two interlocking knot halves.
#[component]
pub fn IconEmptyPlugins(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M4 8 C4 6 6 5 8 6 L13 11" }
            path { d: "M4 8 C4 10 6 11 8 10 L13 6" }
            path { d: "M20 16 C20 18 18 19 16 18 L11 13" }
            path { d: "M20 16 C20 14 18 13 16 14 L11 18" }
            path { d: "M10 10 L14 14" }
        },
        s,
        c,
    )
}

// ──────────────── Glyph-replacement action icons ────────────────

#[component]
pub fn IconRefresh(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // ouroboros-curved circular arrows
            path { d: "M20 12 A8 8 0 1 1 6.5 6.5" }
            path { d: "M4 12 A8 8 0 1 0 17.5 17.5" }
            path { d: "M20 5 V8 H17" }
            path { d: "M4 19 V16 H7" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconArrowLeft(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M19 12 H5" }
            path { d: "M12 5 L5 12 L12 19" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconArrowRight(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M5 12 H19" }
            path { d: "M12 5 L19 12 L12 19" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconCheck(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M5 12.5 L10 17.5 L19.5 6.5" } }, s, c)
}

/// Copy — overlapping wax tablets.
#[component]
pub fn IconCopy(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // back tablet
            path { d: "M9 9 H17 C18 9 19 10 19 11 V19 C19 20 18 21 17 21 H9 C8 21 7 20 7 19 V11 C7 10 8 9 9 9 Z" }
            // front tablet
            path { d: "M5 5 H13 C14 5 15 6 15 7 V8" }
            // stylus marks
            path { d: "M10 14 H16" }
            path { d: "M10 17 H14" }
        },
        s,
        c,
    )
}

/// Trash — delete (universal affordance).
#[component]
pub fn IconTrash(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M4 7 H20" }
            path { d: "M9 7 V5.5 A1.5 1.5 0 0 1 10.5 4 H13.5 A1.5 1.5 0 0 1 15 5.5 V7" }
            path { d: "M6 7 L7 20 A1.5 1.5 0 0 0 8.5 21.5 H15.5 A1.5 1.5 0 0 0 17 20.5 L18 7" }
            path { d: "M10 11 V17" }
            path { d: "M14 11 V17" }
        },
        s,
        c,
    )
}

/// Edit — stylus over wax tablet.
/// Edit — pencil (universal affordance).
#[component]
pub fn IconEdit(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // pencil body
            path { d: "M4 20 L8 19 L20 7 L17 4 L5 16 Z" }
            // pencil tip line
            path { d: "M14 5 L19 10" }
        },
        s,
        c,
    )
}

/// Send — herald's dart (winged arrow).
#[component]
pub fn IconSend(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // shaft
            path { d: "M4 20 L20 4" }
            // arrowhead
            path { d: "M14 4 H20 V10" }
            // wings (herald's dart)
            path { d: "M10 6 C8 6 6 7 5 9" }
            path { d: "M8 8 C7 8.5 6 9.5 5.5 10.5" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconMenu(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M4 7 H20" }
            path { d: "M4 12 H20" }
            path { d: "M4 17 H20" }
        },
        s,
        c,
    )
}

/// File — single papyrus sheet.
#[component]
pub fn IconFile(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M6 3 H14 L19 8 V19 C19 20 18 21 17 21 H7 C6 21 5 20 5 19 V5 C5 4 6 3 7 3 Z" }
            path { d: "M14 3 V8 H19" }
            path { d: "M8 13 H15" }
            path { d: "M8 17 H13" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconFullscreen(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M4 9 V5 C4 4.5 4.5 4 5 4 H9" }
            path { d: "M15 4 H19 C19.5 4 20 4.5 20 5 V9" }
            path { d: "M20 15 V19 C20 19.5 19.5 20 19 20 H15" }
            path { d: "M9 20 H5 C4.5 20 4 19.5 4 19 V15" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconMinimize(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M9 4 V7 C9 8 8 9 7 9 H4" }
            path { d: "M15 4 V7 C15 8 16 9 17 9 H20" }
            path { d: "M20 15 V18 C20 19 19 20 18 20 H15" }
            path { d: "M4 15 V18 C4 19 5 20 6 20 H9" }
        },
        s,
        c,
    )
}

/// Globe — sphere + meridian + equator (universal affordance).
#[component]
pub fn IconGlobe(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // sphere
            circle { cx: "12", cy: "12", r: "9" }
            // equator
            path { d: "M3 12 H21" }
            // meridian
            path { d: "M12 3 C8 7 8 17 12 21 C16 17 16 7 12 3 Z" }
        },
        s,
        c,
    )
}

#[component]
pub fn IconPlay(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(rsx! { path { d: "M7 5 L19 12 L7 19 Z" } }, s, c)
}

// ──────────────── Mythology motif icons ────────────────
// Classical motifs for brand, sections, and accents. Drawn in the
// same smooth line style so they read as one family with the UI icons.

/// Owl of Athena — the brand mark / wisdom motif.
#[component]
pub fn IconOwl(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // facial disc / body
            path { d: "M12 3.4 C7.8 3.4 5 6.2 5 10.2 V12.6 A7 7 0 0 0 19 12.6 V10.2 C19 6.2 16.2 3.4 12 3.4 Z" }
            // ear tufts rising from brows
            path { d: "M6.6 9.2 C6.2 6.8 7.1 5.2 8.6 4.9 C9.6 5.7 10 7 9.7 8.4" }
            path { d: "M17.4 9.2 C17.8 6.8 16.9 5.2 15.4 4.9 C14.4 5.7 14 7 14.3 8.4" }
            // eyes
            circle { cx: "9", cy: "10.6", r: "2.4" }
            circle { cx: "15", cy: "10.6", r: "2.4" }
            circle { cx: "9", cy: "10.6", r: "0.75", fill: c }
            circle { cx: "15", cy: "10.6", r: "0.75", fill: c }
            // beak
            path { d: "M12 12.4 L10.9 14.2 H13.1 Z" }
            // talons
            path { d: "M9.6 19.6 V21" }
            path { d: "M12 20 V21.4" }
            path { d: "M14.4 19.6 V21" }
        },
        s,
        c,
    )
}

/// Laurel branch — achievement / sections.
#[component]
pub fn IconLaurel(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // central stem
            path { d: "M12 21 V6" }
            // paired leaves, curved (almond-shaped)
            path { d: "M12 9 C10 9 8.5 8 8 6 C10 5.7 11.6 6.4 12 9 Z" }
            path { d: "M12 9 C14 9 15.5 8 16 6 C14 5.7 12.4 6.4 12 9 Z" }
            path { d: "M12 14 C9.7 14 8 12.8 7.4 10.6 C9.7 10.2 11.5 11.1 12 14 Z" }
            path { d: "M12 14 C14.3 14 16 12.8 16.6 10.6 C14.3 10.2 12.5 11.1 12 14 Z" }
            path { d: "M12 19 C9.4 19 7.5 17.6 6.8 15 C9.4 14.5 11.5 15.6 12 19 Z" }
            path { d: "M12 19 C14.6 19 16.5 17.6 17.2 15 C14.6 14.5 12.5 15.6 12 19 Z" }
        },
        s,
        c,
    )
}

/// Classical column — structure / workspaces.
#[component]
pub fn IconColumn(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // capital (abacus + echinus)
            path { d: "M4 6 H20" }
            path { d: "M5.5 6 L7 4 H17 L18.5 6" }
            // shaft with fluting
            path { d: "M7 6 V18" }
            path { d: "M10.5 6 V18" }
            path { d: "M13.5 6 V18" }
            path { d: "M17 6 V18" }
            // base
            path { d: "M4 18 H20" }
            path { d: "M5 18 L3.5 20.5 H20.5 L19 18" }
        },
        s,
        c,
    )
}

/// Aegis / shield — protection, security, swarm guard.
#[component]
pub fn IconAegis(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // shield outline
            path { d: "M12 2 L20 5 V11 C20 16 16.5 20 12 22 C7.5 20 4 16 4 11 V5 Z" }
            // Gorgon face — eyes
            circle { cx: "9.5", cy: "9.5", r: "0.9", fill: c }
            circle { cx: "14.5", cy: "9.5", r: "0.9", fill: c }
            // mouth (curved frown)
            path { d: "M10 13.5 C11 12.5 13 12.5 14 13.5" }
        },
        s,
        c,
    )
}

/// Amphora — storage / sessions.
#[component]
pub fn IconAmphora(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // neck + rim
            path { d: "M9 3 H15" }
            path { d: "M10 3 C10 5 7.5 6 7.5 9 A5.5 5.5 0 0 0 16.5 9 C16.5 6 14 5 14 3" }
            // handles
            path { d: "M7.5 9 C5.8 9 4.5 10 4.5 11.5" }
            path { d: "M16.5 9 C18.2 9 19.5 10 19.5 11.5" }
            // body band
            path { d: "M6.5 14 H17.5" }
            // foot
            path { d: "M10 19 H14 L13.4 21 H10.6 Z" }
        },
        s,
        c,
    )
}

/// Corinthian helmet — strategy / agents.
#[component]
pub fn IconHelmet(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // dome
            path { d: "M5.5 13.5 V12 C5.5 8.2 8.2 5.5 12 5.5 C15.8 5.5 18.5 8.2 18.5 12 V13.5" }
            // cheek guards curling in
            path { d: "M5.5 13.5 C5.5 16 7 17.5 9 17.5 V13" }
            path { d: "M18.5 13.5 C18.5 16 17 17.5 15 17.5 V13" }
            // nose guard
            path { d: "M12 8 V16.5" }
            // crest
            path { d: "M8 6.5 C9 3.5 15 3.5 16 6.5" }
        },
        s,
        c,
    )
}

/// Scroll — documents / plans.
#[component]
pub fn IconScroll(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // rolled body
            path { d: "M7 4 H16 C17.5 4 18.5 5 18.5 6.5 V16" }
            path { d: "M7 4 C5.5 4 4.5 5 4.5 6.5 V8 H8 V6.5 C8 5 7 4 7 4 Z" }
            // unrolled sheet
            path { d: "M18.5 16 C18.5 17.5 17.5 18.5 16 18.5 H8 C6.5 18.5 5.5 17.5 5.5 16 V8 H16.5 V16 C16.5 17.5 17.5 18.5 18.5 18.5 Z" }
            // text lines
            path { d: "M9 11 H14" }
            path { d: "M9 14 H13" }
        },
        s,
        c,
    )
}

/// Greek-key (meander) tile — decorative accent.
#[component]
pub fn IconMeander(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // continuous meander key
            path { d: "M3 20 V9 H10 V16 H6.5 V12 H8.5" }
            path { d: "M21 4 V15 H14 V8 H17.5 V12 H15.5" }
        },
        s,
        c,
    )
}
