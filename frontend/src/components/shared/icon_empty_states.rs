use dioxus::prelude::*;

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
