use super::inline_svg;
use dioxus::prelude::*;

// ──────────────── Mythology motif icons ────────────────
// Classical motifs for brand, sections, and accents. Drawn in the
// same smooth line style so they read as one family with the UI icons.

/// Owl of Athena — the brand mark / wisdom motif. The engraved line body keeps
/// `currentColor`; the pupils are gold-leaf (the lamp lit in the wise eye).
#[component]
pub fn IconOwl(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(18);
    let c = color.as_deref().unwrap_or("currentColor");
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            // facial disc / body
            path { d: "M12 3.4 C7.8 3.4 5 6.2 5 10.2 V12.6 A7 7 0 0 0 19 12.6 V10.2 C19 6.2 16.2 3.4 12 3.4 Z" }
            // ear tufts rising from brows
            path { d: "M6.6 9.2 C6.2 6.8 7.1 5.2 8.6 4.9 C9.6 5.7 10 7 9.7 8.4" }
            path { d: "M17.4 9.2 C17.8 6.8 16.9 5.2 15.4 4.9 C14.4 5.7 14 7 14.3 8.4" }
            // eyes — engraved rings
            circle { cx: "9", cy: "10.6", r: "2.4" }
            circle { cx: "15", cy: "10.6", r: "2.4" }
            // pupils — gold-leaf focal
            circle { cx: "9", cy: "10.6", r: "0.85", fill: gold }
            circle { cx: "15", cy: "10.6", r: "0.85", fill: gold }
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

/// Athena — compact owl-face brand glyph for the titlebar "Athena" toggle.
/// A simplified Owl of Athena (facial disc, two eyes, ear tufts, beak) tuned to
/// read as the Athena brand mark at 16px. Distinct from the richer [`IconOwl`]
/// used at larger sizes for toasts.
#[component]
pub fn IconAthena(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            // facial disc / head
            path { d: "M12 4 C7.9 4 5.5 6.6 5.5 10.4 V13 A6.5 6.5 0 0 0 18.5 13 V10.4 C18.5 6.6 16.1 4 12 4 Z" }
            // ear tufts rising from the brows
            path { d: "M7.2 9 C6.9 7 7.6 5.6 8.9 5.3 C9.7 6 10 7.1 9.8 8.3" }
            path { d: "M16.8 9 C17.1 7 16.4 5.6 15.1 5.3 C14.3 6 14 7.1 14.2 8.3" }
            // eyes — engraved rings
            circle { cx: "9.4", cy: "10.6", r: "2.1" }
            circle { cx: "14.6", cy: "10.6", r: "2.1" }
            // pupils — gold-leaf focal
            circle { cx: "9.4", cy: "10.6", r: "0.78", fill: gold }
            circle { cx: "14.6", cy: "10.6", r: "0.78", fill: gold }
            // beak
            path { d: "M12 12.3 L11.1 13.8 H12.9 Z" }
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
    let gold = "var(--goldLeaf)";
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
            // crown berry at the apex — gold-leaf focal
            circle { cx: "12", cy: "5", r: "1", fill: gold }
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
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            // shield outline
            path { d: "M12 2 L20 5 V11 C20 16 16.5 20 12 22 C7.5 20 4 16 4 11 V5 Z" }
            // Gorgon face — engraved eye sockets
            circle { cx: "9.5", cy: "9.5", r: "1.4" }
            circle { cx: "14.5", cy: "9.5", r: "1.4" }
            // Gorgon eyes — gold-leaf focal (the Gorgon's petrifying gaze)
            circle { cx: "9.5", cy: "9.5", r: "0.95", fill: gold }
            circle { cx: "14.5", cy: "9.5", r: "0.95", fill: gold }
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
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            // neck + rim
            path { d: "M9 3 H15" }
            path { d: "M10 3 C10 5 7.5 6 7.5 9 A5.5 5.5 0 0 0 16.5 9 C16.5 6 14 5 14 3" }
            // neck band — gold-leaf focal (the amphora's maker-seal at the throat)
            path { d: "M10 3.7 H14 V4.9 H10 Z", fill: gold, stroke: "none" }
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
    let gold = "var(--goldLeaf)";
    inline_svg(
        rsx! {
            // dome
            path { d: "M5.5 13.5 V12 C5.5 8.2 8.2 5.5 12 5.5 C15.8 5.5 18.5 8.2 18.5 12 V13.5" }
            // cheek guards curling in
            path { d: "M5.5 13.5 C5.5 16 7 17.5 9 17.5 V13" }
            path { d: "M18.5 13.5 C18.5 16 17 17.5 15 17.5 V13" }
            // nose guard
            path { d: "M12 8 V16.5" }
            // crest plume
            path { d: "M8 6.5 C9 3.5 15 3.5 16 6.5" }
            // crest-mount rivet at the brow — gold-leaf focal
            circle { cx: "12", cy: "6.5", r: "1", fill: gold }
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
    let gold = "var(--goldLeaf)";
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
            // rolled-rod end boss — gold-leaf focal (the wax-sealed scroll cap)
            circle { cx: "18.5", cy: "16", r: "1.1", fill: gold }
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

/// The Θ seal — the bronze theta-disc brand mark. One instance per surface
/// (titlebar, modal header, OwlMark halo, welcome pendant, new-session button).
/// The outer disc is engraved gold; the central theta bar + dot are gold-leaf
/// filled; a faint lapis tick sits at the crown as an instrument degree-mark.
/// Drawn on its own 24×24 viewBox (not via `inline_svg`, which hard-codes
/// `currentColor` as the only stroke — the seal needs gold leaf + lapis
/// together, so it composes the SVG directly).
#[component]
pub fn IconSeal(size: Option<u8>, color: Option<String>) -> Element {
    let s = size.unwrap_or(16);
    let c = color.as_deref().unwrap_or("currentColor");
    let gold = "var(--goldLeaf)";
    let lapis = "var(--accentLapis)";
    let size_str = format!("{s}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: c,
            stroke_width: "1.5",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {size_str}; height: {size_str}; display: inline-block; vertical-align: middle; overflow: visible;",
            // engraved outer ring (the disc rim)
            circle { cx: "12", cy: "12", r: "10", stroke: c, fill: "none" }
            // inner engraved guide ring (the almucantar)
            circle { cx: "12", cy: "12", r: "6.5", stroke: c, fill: "none", stroke_opacity: "0.45" }
            // theta bar (gold-leaf)
            path { d: "M5 12 H19", stroke: gold, stroke_width: "2" }
            // theta dot (gold-leaf filled)
            circle { cx: "12", cy: "12", r: "2.4", fill: gold, stroke: "none" }
            // faint lapis degree-tick at the crown (instrument mark)
            path { d: "M12 1.5 V3.5", stroke: lapis, stroke_width: "1" }
        }
    }
}
