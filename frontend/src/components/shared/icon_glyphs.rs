use super::inline_svg;
use dioxus::prelude::*;

// ──────────────── Glyph-replacement action icons ────────────────

#[component]
pub fn IconRefresh(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M20 11 A8 8 0 1 0 18.4 16.3" }
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

/// Copy — two stacked sheets.
#[component]
pub fn IconCopy(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "9", y: "9", width: "12", height: "12", rx: "2" }
            path { d: "M5 15 H4 A2 2 0 0 1 2 13 V4 A2 2 0 0 1 4 2 H13 A2 2 0 0 1 15 4 V5" }
        },
        s,
        c,
    )
}

/// Trash — delete.
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

/// Edit — pencil.
#[component]
pub fn IconEdit(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M21.174 6.812 A1 1 0 0 0 17.188 2.825 L3.842 16.174 A2 2 0 0 0 3.342 17.004 L2.021 21.356 A0.5 0.5 0 0 0 2.644 21.978 L6.997 20.658 A2 2 0 0 0 7.827 20.161 Z" }
            path { d: "M15 5 L19 9" }
        },
        s,
        c,
    )
}

/// Send — paper plane.
#[component]
pub fn IconSend(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M22 2 L15 22 L11 13 L2 9 Z" }
            path { d: "M22 2 L11 13" }
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

/// File — single sheet with folded corner.
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

/// Fullscreen — corner brackets pointing outward.
#[component]
pub fn IconFullscreen(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M4 9 V4 H9" }
            path { d: "M15 4 H20 V9" }
            path { d: "M20 15 V20 H15" }
            path { d: "M9 20 H4 V15" }
        },
        s,
        c,
    )
}

/// Minimize — corner brackets pointing inward.
#[component]
pub fn IconMinimize(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M9 4 L9 9 L4 9" }
            path { d: "M15 4 L15 9 L20 9" }
            path { d: "M15 20 L15 15 L20 15" }
            path { d: "M9 20 L9 15 L4 15" }
        },
        s,
        c,
    )
}

/// Globe — sphere + meridian + equator.
#[component]
pub fn IconGlobe(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "12", cy: "12", r: "9" }
            path { d: "M3 12 H21" }
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

/// Star — priority / achievement.
#[component]
pub fn IconStar(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path {
                d: "M11.525 2.295 A0.53 0.53 0 0 1 12.475 2.295 L14.785 6.974 A2.123 2.123 0 0 0 16.38 8.134 L21.546 8.89 A0.53 0.53 0 0 1 21.84 9.794 L18.104 13.432 A2.123 2.123 0 0 0 17.493 15.31 L18.375 20.45 A0.53 0.53 0 0 1 17.604 21.01 L12.986 18.582 A2.122 2.122 0 0 0 11.014 18.582 L6.396 21.01 A0.53 0.53 0 0 1 5.625 20.45 L6.507 15.31 A2.123 2.123 0 0 0 5.896 13.432 L2.16 9.794 A0.53 0.53 0 0 1 2.454 8.89 L7.62 8.134 A2.123 2.123 0 0 0 9.215 6.974 Z"
            }
        },
        s,
        c,
    )
}

/// Warning — alert triangle.
#[component]
pub fn IconWarning(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M10.29 3.86 L1.82 18 A2 2 0 0 0 3.53 21 H20.47 A2 2 0 0 0 22.18 18 L13.71 3.86 A2 2 0 0 0 10.29 3.86 Z" }
            path { d: "M12 9 V14" }
            path { d: "M12 17.5 H12.01" }
        },
        s,
        c,
    )
}

/// Keyboard — shortcuts reference.
#[component]
pub fn IconKeyboard(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "2", y: "4", width: "20", height: "16", rx: "2" }
            path { d: "M6 8 H6.01 M10 8 H10.01 M14 8 H14.01 M18 8 H18.01" }
            path { d: "M6 12 H6.01 M10 12 H10.01 M14 12 H14.01" }
            path { d: "M18 12 H18.01 M6 16 H18.01 M10 16 H10.01 M14 16 H14.01" }
        },
        s,
        c,
    )
}

/// Smartphone — mobile mirror.
#[component]
pub fn IconSmartphone(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "5", y: "2", width: "14", height: "20", rx: "2" }
            path { d: "M12 18 H12.01" }
        },
        s,
        c,
    )
}

/// Window maximize — two overlapping frames.
#[component]
pub fn IconWindowMaximize(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "4", y: "4", width: "16", height: "16", rx: "1.5" }
            path { d: "M4 9 H20" }
        },
        s,
        c,
    )
}

/// Window restore — overlapping frames.
#[component]
pub fn IconWindowRestore(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "8", y: "8", width: "12", height: "12", rx: "1.5" }
            path { d: "M4 16 V6 A2 2 0 0 1 6 4 H16" }
        },
        s,
        c,
    )
}
