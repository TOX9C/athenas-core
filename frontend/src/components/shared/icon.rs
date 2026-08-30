use dioxus::prelude::*;

// ──────────────── shared icon SVG constants ──────────────
// Professional geometric icon set. Every glyph is drawn on a
// 24×24 grid with a consistent 1.75px stroke and round caps/joins,
// so the whole UI reads as one system at any size. Color is
// inherited from the element scope or the `color` prop, so one icon
// works on light and dark themes.

/// Render an SVG with the default 24x24 viewBox.
fn inline_svg(children: Element, size: u8, color: &str) -> Element {
    let size_str = format!("{size}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: color,
            stroke_width: "1.75",
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

// ──────────────── Navigation / Section icons ────────────────

/// Spaces — clean 2×2 grid.
#[component]
pub fn IconSpaces(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "3.5", y: "3.5", width: "7", height: "7", rx: "1.5" }
            rect { x: "13.5", y: "3.5", width: "7", height: "7", rx: "1.5" }
            rect { x: "13.5", y: "13.5", width: "7", height: "7", rx: "1.5" }
            rect { x: "3.5", y: "13.5", width: "7", height: "7", rx: "1.5" }
        },
        s,
        c,
    )
}

/// Files — stacked documents.
#[component]
pub fn IconFiles(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // back sheet
            path { d: "M8 4.5 H15.5 A2 2 0 0 1 17.5 6.5 V16 A2 2 0 0 1 15.5 18" }
            path { d: "M8 4.5 A2 2 0 0 0 6 6.5 V17.5 A2 2 0 0 0 8 19.5 H14 A2 2 0 0 0 16 17.5" }
            // front sheet
            path { d: "M11 2.5 H18.5 A2 2 0 0 1 20.5 4.5 V15 A2 2 0 0 1 18.5 17 H11 A2 2 0 0 1 9 15 V4.5 A2 2 0 0 1 11 2.5 Z" }
            // text lines
            path { d: "M12.5 7 H17" }
            path { d: "M12.5 10.5 H16" }
        },
        s,
        c,
    )
}

/// Agents — person (user) icon.
#[component]
pub fn IconAgents(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M20 21 V19 A4 4 0 0 0 16 15 H8 A4 4 0 0 0 4 19 V21" }
            circle { cx: "12", cy: "7", r: "4" }
        },
        s,
        c,
    )
}

/// Plugins — interlocking puzzle piece.
#[component]
pub fn IconPlugins(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M14 7V5a2 2 0 0 0-4 0v2" }
            path { d: "M14 7h2a2 2 0 0 1 2 2v2" }
            path { d: "M10 7H8a2 2 0 0 0-2 2v2" }
            path { d: "M6 13h12" }
            path { d: "M10 13v2a2 2 0 0 0 4 0v-2" }
            path { d: "M6 13v5a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2v-5" }
        },
        s,
        c,
    )
}

/// Settings — standard 8-tooth gear with center bore.
#[component]
pub fn IconSettings(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path {
                d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
            }
            circle { cx: "12", cy: "12", r: "3" }
        },
        s,
        c,
    )
}

/// Tune — three quiet controls for appearance and preferences.
#[component]
pub fn IconTune(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M4 6 H20" }
            path { d: "M4 12 H20" }
            path { d: "M4 18 H20" }
            circle { cx: "9", cy: "6", r: "1.8", fill: "var(--bgSecondary)" }
            circle { cx: "15", cy: "12", r: "1.8", fill: "var(--bgSecondary)" }
            circle { cx: "11", cy: "18", r: "1.8", fill: "var(--bgSecondary)" }
        },
        s,
        c,
    )
}

/// Search — smooth lens.
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

/// Bell — notification bell.
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

/// Terminal — prompt glyph.
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

/// Zap — lightning bolt.
#[component]
pub fn IconZap(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! { path { d: "M13 2 L5 13 H11 L9 22 L19 9 H13 L15 2 Z" } },
        s,
        c,
    )
}

/// Swarm — hub-and-spoke network: a central node connected to satellite
/// nodes by lines. Reads as a coordinated agent swarm at 16px.
#[component]
pub fn IconSwarm(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            path { d: "M12 12 V5" }
            path { d: "M12 12 L18 8" }
            path { d: "M12 12 L18 16" }
            path { d: "M12 12 L6 16" }
            circle { cx: "12", cy: "12", r: "2.1", fill: gold }
            circle { cx: "12", cy: "5", r: "1.4", fill: c }
            circle { cx: "18", cy: "8", r: "1.4", fill: c }
            circle { cx: "18", cy: "16", r: "1.4", fill: c }
            circle { cx: "6", cy: "16", r: "1.4", fill: c }
        },
        s,
        c,
    )
}

/// Grid — 2×2 tiles.
#[component]
pub fn IconGrid(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "3.5", y: "3.5", width: "7", height: "7", rx: "1.5" }
            rect { x: "13.5", y: "3.5", width: "7", height: "7", rx: "1.5" }
            rect { x: "13.5", y: "13.5", width: "7", height: "7", rx: "1.5" }
            rect { x: "3.5", y: "13.5", width: "7", height: "7", rx: "1.5" }
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

/// More — three dots.
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

/// Eye — view.
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

/// EyeOff — hidden.
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
    IconGlobe, IconKeyboard, IconMenu, IconMic, IconMinimize, IconPlay, IconRefresh, IconSend,
    IconSmartphone, IconStar, IconTrash, IconWarning, IconWindowMaximize, IconWindowRestore,
};

#[path = "icon_mythology.rs"]
mod icon_mythology;

pub use icon_mythology::{
    IconArchive, IconAthena, IconInfo, IconKanban, IconLaurel, IconList, IconLoop, IconPulse,
    IconSeal, IconShield, IconSparkle,
};
