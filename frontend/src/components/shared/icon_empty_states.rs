use dioxus::prelude::*;

// ──────────────── Line-art empty-state illustrations ────────────────
// Minimal geometric compositions for empty-state screens, drawn in the
// same smooth line style as the main icon set.

fn empty_svg(children: Element, size: u16, color: &str) -> Element {
    let size_str = format!("{size}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: color,
            stroke_width: "1.6",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {size_str}; height: {size_str}; display: inline-block; overflow: visible;",
            {children}
        }
    }
}

/// Empty workspace — 2×2 grid with a plus in one tile.
#[component]
pub fn IconEmptyWorkspace(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            rect { x: "3", y: "3", width: "7.5", height: "7.5", rx: "1.6" }
            rect { x: "13.5", y: "3", width: "7.5", height: "7.5", rx: "1.6" }
            rect { x: "13.5", y: "13.5", width: "7.5", height: "7.5", rx: "1.6" }
            rect { x: "3", y: "13.5", width: "7.5", height: "7.5", rx: "1.6" }
            path { d: "M6.75 13.5 V18.75 M4.125 16.125 H9.375" }
        },
        s,
        c,
    )
}

/// Empty chat — rounded bubble with a sparkle.
#[component]
pub fn IconEmptyChat(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M21 11.5 A8.5 8.5 0 0 1 12.5 20 C11.2 20 9.9 19.7 8.8 19.2 L3 21 L4.6 15.7 A8.5 8.5 0 1 1 21 11.5 Z" }
            path { d: "M9.4 9.9 A1.9 1.9 0 0 0 8.3 8.7 A1.9 1.9 0 0 0 7.1 9.9 A1.9 1.9 0 0 0 8.3 11.1 A1.9 1.9 0 0 0 9.4 9.9 Z" }
            path { d: "M14.8 9.9 A1.9 1.9 0 0 0 13.7 8.7 A1.9 1.9 0 0 0 12.5 9.9 A1.9 1.9 0 0 0 13.7 11.1 A1.9 1.9 0 0 0 14.8 9.9 Z" }
            circle { cx: "17", cy: "9.9", r: "1.15", fill: c, stroke: "none" }
        },
        s,
        c,
    )
}

/// Empty kanban — three columns with cards.
#[component]
pub fn IconEmptyKanban(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
            path { d: "M9 3 V21" }
            path { d: "M15 3 V21" }
            path { d: "M5.5 7.5 H7.5" }
            path { d: "M5.5 11 H7.5" }
            path { d: "M16.5 7.5 H18.5" }
            path { d: "M16.5 11 H18.5" }
        },
        s,
        c,
    )
}

/// Empty swarm — hub-and-spoke network.
#[component]
pub fn IconEmptySwarm(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M12 11 V5.5 M13.4 10.6 L17.4 6.6 M13.4 13.4 L17.4 17.4 M12 13 V18.5 M10.6 13.4 L6.6 17.4 M10.6 10.6 L6.6 6.6" }
            circle { cx: "12", cy: "12", r: "2.2" }
            circle { cx: "12", cy: "4.6", r: "1.5" }
            circle { cx: "18.2", cy: "5.8", r: "1.5" }
            circle { cx: "18.2", cy: "18.2", r: "1.5" }
            circle { cx: "12", cy: "19.4", r: "1.5" }
            circle { cx: "5.8", cy: "18.2", r: "1.5" }
            circle { cx: "5.8", cy: "5.8", r: "1.5" }
        },
        s,
        c,
    )
}

/// Empty notifications — bell with a quiet dot.
#[component]
pub fn IconEmptyNotifications(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M18 8 A6 6 0 0 0 6 8 C6 15 3 17 3 17 H21 C21 17 18 15 18 8 Z" }
            path { d: "M13.7 21 A2 2 0 0 1 10.3 21" }
            circle { cx: "12", cy: "10.5", r: "1.4", fill: c, stroke: "none" }
        },
        s,
        c,
    )
}

/// Empty plugins — two interlocking puzzle pieces.
#[component]
pub fn IconEmptyPlugins(size: Option<u16>, color: Option<String>) -> Element {
    let s = size.unwrap_or(48);
    let c = color.as_deref().unwrap_or("var(--textDim)");
    empty_svg(
        rsx! {
            path { d: "M8 3 H16 A1.5 1.5 0 0 1 17.5 4.5 V7 A2 2 0 0 1 15.5 9 V15 A2 2 0 0 1 17.5 17 V19.5 A1.5 1.5 0 0 1 16 21 H8 A1.5 1.5 0 0 1 6.5 19.5 V17 A2 2 0 0 1 8.5 15 V9 A2 2 0 0 1 6.5 7 V4.5 A1.5 1.5 0 0 1 8 3 Z" }
            path { d: "M12 7 V9 M12 15 V17" }
        },
        s,
        c,
    )
}
