use super::inline_svg;
use dioxus::prelude::*;
// The Athena mark is her spear drawn as a solid lambda: a diamond point at
// the apex above two strong legs. Solid fills — no strokes — so it stays
// bold and legible from 15px chrome up to 1024px app icons. Keep this
// aligned with CoreMark, athena.svg, and the promo mark so the identity
// remains consistent.

/// Spear legs — solid lambda slash-open at the bottom.
const ATHENA_FRAME: &str = "M12 7.6 L19.2 20 L15.9 20 L12 12.6 L8.1 20 L4.8 20 Z";
/// Spearhead — diamond point floating above the legs.
const ATHENA_CORE: &str = "M12 3.2 L13.3 4.9 L12 6.7 L10.7 4.9 Z";
fn brand_svg(children: Element, size: u8, color: &str) -> Element {
    let size_str = format!("{size}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            "aria-hidden": "true",
            stroke: color,
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {size_str}; height: {size_str}; display: inline-block; vertical-align: middle; overflow: visible;",
            {children}
        }
    }
}

/// Athena — the compact spear-A mark for titlebars and toolbars.
#[component]
pub fn IconAthena(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    brand_svg(
        rsx! {
            path { d: "{ATHENA_FRAME}", fill: c, stroke: "none" }
            path { d: "{ATHENA_CORE}", fill: c, stroke: "none" }
        },
        s,
        c,
    )
}

/// Seal — the larger presentation version of the spear-A mark.
#[component]
pub fn IconSeal(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    brand_svg(
        rsx! {
            path { d: "{ATHENA_FRAME}", fill: c, stroke: "none" }
            path { d: "{ATHENA_CORE}", fill: c, stroke: "none" }
        },
        s,
        c,
    )
}

/// Laurel — a quiet horizontal branch used as a welcome-screen divider.
/// It is intentionally drawn as transparent line art so it never introduces
/// the opaque rectangular canvas of a raster banner.
#[component]
pub fn IconLaurel(size: Option<u8>, color: Option<String>) -> Element {
    let width = size.unwrap_or(180);
    let height = (width as f32 * 0.34).round() as u16;
    let c = color.as_deref().unwrap_or("currentColor");
    rsx! {
        svg {
            view_box: "0 0 160 54",
            fill: "none",
            "aria-hidden": "true",
            stroke: c,
            stroke_width: "1.35",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {width}px; height: {height}px; display: block; overflow: visible;",
            // Stem: a shallow upward sweep keeps the motif aligned with the
            // open, centered welcome composition rather than reading as a bar.
            path { d: "M80 42 C62 40 42 34 20 20" }
            path { d: "M80 42 C98 40 118 34 140 20" }
            path { d: "M80 42 C80 35 80 29 80 23", stroke_opacity: "0.7" }

            // Left leaves.
            path { d: "M63 38 C57 33 55 27 57 22 C63 25 66 31 63 38 Z" }
            path { d: "M51 34 C44 30 41 24 42 19 C49 21 53 27 51 34 Z" }
            path { d: "M39 29 C32 25 29 19 30 14 C37 16 41 22 39 29 Z" }
            path { d: "M28 23 C22 20 18 15 19 11 C25 12 30 17 28 23 Z" }
            path { d: "M69 40 C64 35 63 30 65 26 C70 29 72 35 69 40 Z", stroke_opacity: "0.72" }

            // Right leaves.
            path { d: "M97 38 C103 33 105 27 103 22 C97 25 94 31 97 38 Z" }
            path { d: "M109 34 C116 30 119 24 118 19 C111 21 107 27 109 34 Z" }
            path { d: "M121 29 C128 25 131 19 130 14 C123 16 119 22 121 29 Z" }
            path { d: "M132 23 C138 20 142 15 141 11 C135 12 130 17 132 23 Z" }
            path { d: "M91 40 C96 35 97 30 95 26 C90 29 88 35 91 40 Z", stroke_opacity: "0.72" }
        }
    }
}

/// Info — informational toast / about. Filled-circle affordance.
#[component]
pub fn IconInfo(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            circle { cx: "12", cy: "12", r: "10" }
            path { d: "M12 16 V11" }
            path { d: "M12 8 H12.01" }
        },
        s,
        c,
    )
}

/// Pulse — activity / status (agent working state).
#[component]
pub fn IconPulse(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path {
                d: "M22 12 H19.52 A2 2 0 0 0 17.59 13.46 L15.24 21.82 A0.25 0.25 0 0 1 14.76 21.82 L9.24 2.18 A0.25 0.25 0 0 0 8.76 2.18 L6.41 10.54 A2 2 0 0 1 4.48 12 H2"
            }
        },
        s,
        c,
    )
}

/// Kanban — three board columns.
#[component]
pub fn IconKanban(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
            path { d: "M9 3 V21" }
            path { d: "M15 3 V21" }
        },
        s,
        c,
    )
}

/// Sparkle — achievement / accent mark.
#[component]
pub fn IconSparkle(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path {
                d: "M9.937 15.5 A2 2 0 0 0 8.5 14.063 L2.365 12.481 A0.5 0.5 0 0 1 2.365 11.519 L8.5 9.936 A2 2 0 0 0 9.937 8.5 L11.519 2.365 A0.5 0.5 0 0 1 12.481 2.365 L14.063 8.5 A2 2 0 0 0 15.5 9.937 L21.635 11.519 A0.5 0.5 0 0 1 21.635 12.481 L15.5 14.063 A2 2 0 0 0 14.063 15.5 L12.481 21.635 A0.5 0.5 0 0 1 11.519 21.635 Z"
            }
        },
        s,
        c,
    )
}

/// Shield — protection / security.
#[component]
pub fn IconShield(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path {
                d: "M20 13 C20 18 16.5 20.5 12.34 21.95 A1 1 0 0 1 11.66 21.95 C7.5 20.5 4 18 4 13 V6 A1 1 0 0 1 5 5 C7 5 9.5 3.8 11.24 2.28 A1 1 0 0 1 12.76 2.28 C14.5 3.8 17 5 19 5 A1 1 0 0 1 20 6 Z"
            }
        },
        s,
        c,
    )
}

/// Archive — storage / sessions box.
#[component]
pub fn IconArchive(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            rect { x: "2", y: "3", width: "20", height: "5", rx: "1" }
            path { d: "M4 8 V19 A2 2 0 0 0 6 21 H18 A2 2 0 0 0 20 19 V8" }
            path { d: "M10 12 H14" }
        },
        s,
        c,
    )
}

/// List — documents / plans.
#[component]
pub fn IconList(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path { d: "M8 6 H21" }
            path { d: "M8 12 H21" }
            path { d: "M8 18 H21" }
            path { d: "M3 6 H3.01" }
            path { d: "M3 12 H3.01" }
            path { d: "M3 18 H3.01" }
        },
        s,
        c,
    )
}

/// Loop — continuous cycle / connector.
#[component]
pub fn IconLoop(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(14);
    let c = color.as_deref().unwrap_or("currentColor");
    inline_svg(
        rsx! {
            path {
                d: "M12 12 C10 9.33 8 8 6 8 A4 4 0 1 0 6 16 C8 16 10 14.67 12 12 Z"
            }
            path {
                d: "M12 12 C14 14.67 16 16 18 16 A4 4 0 1 1 18 8 C16 8 14 9.33 12 12 Z"
            }
        },
        s,
        c,
    )
}
