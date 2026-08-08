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

/// Agents — Corinthian helmet, simplified for legibility. Keeps the dome +
/// nose guard + crest; cheek guards reduced to clean single strokes so the
/// helmet silhouette reads at 18px.
#[component]
pub fn IconAgents(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // dome
            path { d: "M6 13 V11.5 C6 8 8.5 5.5 12 5.5 C15.5 5.5 18 8 18 11.5 V13" }
            // cheek guards — single clean strokes down each side
            path { d: "M6 13 V16 H9 V13" }
            path { d: "M18 13 V16 H15 V13" }
            // nose guard (the T of the Corinthian)
            path { d: "M12 9 V16" }
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

/// Settings — toothed gear (universal affordance). A continuous 8-tooth cog
/// body (curved teeth on the rim, no radial spokes) + a center bore ring, so
/// it reads unambiguously as "settings" at 12px instead of a sun/asterisk.
#[component]
pub fn IconSettings(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // 8-tooth cog body: teeth are curved bumps on a continuous rim.
            path {
                d: "M10.325 4.317 C10.751 2.561 13.249 2.561 13.675 4.317 A1.724 1.724 0 0 0 16.248 5.383 C17.791 4.443 19.557 6.209 18.617 7.752 A1.724 1.724 0 0 0 19.682 10.325 C21.438 10.751 21.438 13.249 19.682 13.675 A1.724 1.724 0 0 0 18.617 16.248 C19.557 17.791 17.791 19.557 16.248 18.617 A1.724 1.724 0 0 0 13.675 19.682 C13.249 21.438 10.751 21.438 10.325 19.682 A1.724 1.724 0 0 0 7.752 18.617 C6.209 19.557 4.443 17.791 5.383 16.248 A1.724 1.724 0 0 0 4.318 13.675 C2.562 13.249 2.562 10.751 4.318 10.325 A1.724 1.724 0 0 0 5.383 7.752 C4.443 6.209 6.209 4.443 7.752 5.383 A1.724 1.724 0 0 0 10.325 4.317 Z"
            }
            // center bore
            circle { cx: "12", cy: "12", r: "3" }
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

/// Swarm — hub-and-spoke constellation: a central node connected to satellite
/// nodes by lines. Reads as a coordinated network/swarm at 16px without the
/// busy star of the previous version.
#[component]
pub fn IconSwarm(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            // spokes from the center hub to each satellite
            path { d: "M12 12 L12 5" }
            path { d: "M12 12 L18 8" }
            path { d: "M12 12 L18 16" }
            path { d: "M12 12 L6 16" }
            // central hub — gold-leaf focal (the coordinating core)
            circle { cx: "12", cy: "12", r: "2.1", fill: gold }
            // satellite nodes
            circle { cx: "12", cy: "5", r: "1.4", fill: c }
            circle { cx: "18", cy: "8", r: "1.4", fill: c }
            circle { cx: "18", cy: "16", r: "1.4", fill: c }
            circle { cx: "6", cy: "16", r: "1.4", fill: c }
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

#[path = "icon_empty_states.rs"]
mod icon_empty_states;

pub use icon_empty_states::{
    IconEmptyChat, IconEmptyKanban, IconEmptyNotifications, IconEmptyPlugins, IconEmptySwarm,
    IconEmptyWorkspace,
};

#[path = "icon_glyphs.rs"]
mod icon_glyphs;

pub use icon_glyphs::{
    IconArrowLeft, IconArrowRight, IconCheck, IconCopy, IconEdit, IconFile, IconFullscreen,
    IconGlobe, IconMenu, IconMinimize, IconPlay, IconRefresh, IconSend, IconTrash,
};

#[path = "icon_mythology.rs"]
mod icon_mythology;

pub use icon_mythology::{
    IconAegis, IconAmphora, IconAthena, IconColumn, IconHelmet, IconLaurel, IconMeander, IconOwl,
    IconScroll, IconSeal,
};
