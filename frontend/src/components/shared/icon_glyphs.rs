use super::inline_svg;
use dioxus::prelude::*;

// ──────────────── Glyph-replacement action icons ────────────────

#[component]
pub fn IconRefresh(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // single clockwise circular arrow (~280° sweep) ending in a head
            path { d: "M20 11 A8 8 0 1 0 18.4 16.3" }
            // arrowhead at the open end (top-right)
            path { d: "M15.5 4.5 H20 V9" }
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

/// Fullscreen — four corner brackets pointing outward (expand frame). The
/// mirror of [`IconMinimize`]. Straight L-brackets so it reads unambiguously
/// as "expand" at 16px.
#[component]
pub fn IconFullscreen(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // top-left bracket: out from corner along top, then down the left
            path { d: "M4 9 V4 H9" }
            // top-right bracket
            path { d: "M15 4 H20 V9" }
            // bottom-right bracket
            path { d: "M20 15 V20 H15" }
            // bottom-left bracket
            path { d: "M9 20 H4 V15" }
        },
        s,
        c,
    )
}

/// Minimize — four corner brackets pointing inward toward center (collapse
/// frame), the mirror of [`IconFullscreen`]. Replaces the previous inward-curl
/// pinwheel geometry that read wrong. Where fullscreen reaches out to the
/// corners, minimize reaches in from the corners toward center.
#[component]
pub fn IconMinimize(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            // top-left bracket: in from corner along top, then down toward center
            path { d: "M9 4 L9 9 L4 9" }
            // top-right bracket
            path { d: "M15 4 L15 9 L20 9" }
            // bottom-right bracket
            path { d: "M15 20 L15 15 L20 15" }
            // bottom-left bracket
            path { d: "M9 20 L9 15 L4 15" }
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
