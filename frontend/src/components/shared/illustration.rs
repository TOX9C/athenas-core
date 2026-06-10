use dioxus::prelude::*;

// ────────────────── Empty-state line-art illustrations ──────────────────
// Larger, two-tone compositions in a black-figure / engraved style. Most
// strokes use --textDim; a single highlight stroke uses --accent so the art
// carries the gold identity. Rendered inside `EmptyState`.

fn illo(children: Element) -> Element {
    rsx! {
        svg {
            view_box: "0 0 120 96",
            fill: "none",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: 132px; height: 106px; display: block;",
            {children}
        }
    }
}

/// Which illustration an empty state should show.
#[derive(Clone, Copy, PartialEq)]
pub enum EmptyArt {
    Workspace,
    Sessions,
    Kanban,
    Swarm,
    Notifications,
    Plugins,
    Files,
    Agents,
    Generic,
}

fn art_for(kind: EmptyArt) -> Element {
    match kind {
        EmptyArt::Workspace | EmptyArt::Generic => IlloOwlBranch(),
        EmptyArt::Sessions => IlloScroll(),
        EmptyArt::Kanban => IlloTemple(),
        EmptyArt::Swarm => IlloConstellation(),
        EmptyArt::Notifications => IlloSleepingOwl(),
        EmptyArt::Plugins => IlloLaurelWreath(),
        EmptyArt::Files => IlloAmphora(),
        EmptyArt::Agents => IlloHelmet(),
    }
}

/// Standard empty-state block: illustration + headline + hint + optional action.
#[component]
pub fn EmptyState(
    kind: EmptyArt,
    title: String,
    hint: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "animate-rise",
            style: "flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 18px; padding: 32px; text-align: center;",
            div { style: "opacity: 0.9;", {art_for(kind)} }
            div { style: "display: flex; flex-direction: column; gap: 6px; align-items: center;",
                div {
                    style: "font-family: var(--font-display); font-size: 22px; font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                    "{title}"
                }
                if let Some(h) = hint {
                    div { style: "font-size: 13px; color: var(--textMuted); max-width: 320px; line-height: 1.5;", "{h}" }
                }
            }
            {children}
        }
    }
}

/// Brand owl mark — gold, for welcome / chat / thinking. `size` in px.
#[component]
pub fn OwlMark(size: Option<u16>) -> Element {
    let s = size.unwrap_or(20);
    let sz = format!("{s}px");
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "var(--accent)",
            stroke_width: "1.4",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {sz}; height: {sz}; display: inline-block; vertical-align: middle;",
            // body / facial disc
            path { d: "M12 3.4c-4.2 0-7.2 3-7.2 7.2v2.4a7.2 7.2 0 0 0 14.4 0v-2.4c0-4.2-3-7.2-7.2-7.2z" }
            // brows that rise into ear tufts
            path { d: "M6.7 9.4c-.4-2.4.5-4.1 2-4.4 1.1.8 1.4 2.2 1.1 3.6" }
            path { d: "M17.3 9.4c.4-2.4-.5-4.1-2-4.4-1.1.8-1.4 2.2-1.1 3.6" }
            // eyes + pupils
            circle { cx: "9", cy: "10.8", r: "2.4" }
            circle { cx: "15", cy: "10.8", r: "2.4" }
            circle { cx: "9", cy: "10.8", r: "0.75", fill: "var(--accent)", stroke: "none" }
            circle { cx: "15", cy: "10.8", r: "0.75", fill: "var(--accent)", stroke: "none" }
            // beak
            path { d: "M12 12.6l-1.1 1.8h2.2z" }
            // talons
            path { d: "M9.8 20.7v1.4M12 21.1v1.4M14.2 20.7v1.4" }
        }
    }
}

// ── Individual illustrations ────────────────────────────────────────────────

/// Owl perched on a branch — workspaces / generic.
pub fn IlloOwlBranch() -> Element {
    illo(rsx! {
        // branch
        path { d: "M14 72h92", stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.6" }
        path { d: "M86 72c8-2 14-6 20-12", stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.5" }
        // owl body
        g { stroke: "var(--accent)", stroke_width: "1.6",
            path { d: "M44 32c0-9 7-16 16-16s16 7 16 16v16a16 16 0 0 1-32 0z" }
            // brows rising into ear tufts
            path { d: "M47.5 34c-.8-5 1-8.4 4-9.4 2.2 1.6 2.9 4.6 2.3 7.4" }
            path { d: "M72.5 34c.8-5-1-8.4-4-9.4-2.2 1.6-2.9 4.6-2.3 7.4" }
            circle { cx: "53", cy: "36", r: "4.6" }
            circle { cx: "67", cy: "36", r: "4.6" }
            path { d: "M60 39.5l-2.5 4h5z" }
            path { d: "M52 64v6M60 65v6M68 64v6" }
        }
        circle { cx: "53", cy: "36", r: "1.4", fill: "var(--accent)", stroke: "none" }
        circle { cx: "67", cy: "36", r: "1.4", fill: "var(--accent)", stroke: "none" }
    })
}

/// Unrolled scroll — sessions / history.
pub fn IlloScroll() -> Element {
    illo(rsx! {
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            path { d: "M34 26h46a6 6 0 0 1 6 6v34" }
            path { d: "M34 26a6 6 0 0 0-6 6v4h12v-4a6 6 0 0 0-6-6z" }
            path { d: "M86 66a6 6 0 0 1-6 6H40a6 6 0 0 1-6-6V36h40v30a6 6 0 0 0 6 6z" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.6",
            line { x1: "44", y1: "46", x2: "70", y2: "46" }
            line { x1: "44", y1: "54", x2: "70", y2: "54" }
            line { x1: "44", y1: "62", x2: "60", y2: "62" }
        }
    })
}

/// Temple façade columns — kanban board.
pub fn IlloTemple() -> Element {
    illo(rsx! {
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.7",
            path { d: "M28 30l32-12 32 12" }
            line { x1: "24", y1: "30", x2: "96", y2: "30" }
            line { x1: "26", y1: "74", x2: "94", y2: "74" }
            line { x1: "22", y1: "80", x2: "98", y2: "80" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.6",
            line { x1: "36", y1: "34", x2: "36", y2: "74" }
            line { x1: "52", y1: "34", x2: "52", y2: "74" }
            line { x1: "68", y1: "34", x2: "68", y2: "74" }
            line { x1: "84", y1: "34", x2: "84", y2: "74" }
        }
    })
}

/// Constellation network — swarm.
pub fn IlloConstellation() -> Element {
    illo(rsx! {
        g { stroke: "var(--textDim)", stroke_width: "1.2", opacity: "0.55",
            line { x1: "60", y1: "48", x2: "60", y2: "24" }
            line { x1: "60", y1: "48", x2: "88", y2: "36" }
            line { x1: "60", y1: "48", x2: "90", y2: "64" }
            line { x1: "60", y1: "48", x2: "60", y2: "76" }
            line { x1: "60", y1: "48", x2: "32", y2: "64" }
            line { x1: "60", y1: "48", x2: "30", y2: "36" }
        }
        g { stroke: "var(--textDim)", stroke_width: "1.4", fill: "var(--bg)",
            circle { cx: "60", cy: "24", r: "4" }
            circle { cx: "88", cy: "36", r: "4" }
            circle { cx: "90", cy: "64", r: "4" }
            circle { cx: "60", cy: "76", r: "4" }
            circle { cx: "32", cy: "64", r: "4" }
            circle { cx: "30", cy: "36", r: "4" }
        }
        circle { cx: "60", cy: "48", r: "7", stroke: "var(--accent)", stroke_width: "1.7", fill: "var(--bg)" }
        circle { cx: "60", cy: "48", r: "2", fill: "var(--accent)", stroke: "none" }
    })
}

/// Sleeping owl — no notifications.
pub fn IlloSleepingOwl() -> Element {
    illo(rsx! {
        g { stroke: "var(--textDim)", stroke_width: "1.6",
            path { d: "M44 38c0-9 7-16 16-16s16 7 16 16v12a16 16 0 0 1-32 0z" }
            // brow tufts
            path { d: "M47.5 40c-.8-5 1-8.4 4-9.4 2.2 1.6 2.9 4.6 2.3 7.4" }
            path { d: "M72.5 40c.8-5-1-8.4-4-9.4-2.2 1.6-2.9 4.6-2.3 7.4" }
            // closed eyes (downward arcs)
            path { d: "M49 41c1.6 2.2 6.4 2.2 8 0" }
            path { d: "M63 41c1.6 2.2 6.4 2.2 8 0" }
            path { d: "M60 45l-2 3h4z", stroke: "var(--accent)" }
        }
        // zzz
        g { stroke: "var(--accent)", stroke_width: "1.4", opacity: "0.8",
            path { d: "M80 30h6l-6 6h6" }
            path { d: "M88 22h4l-4 4h4" }
        }
    })
}

/// Laurel wreath — plugins.
pub fn IlloLaurelWreath() -> Element {
    illo(rsx! {
        g { stroke: "var(--accent)", stroke_width: "1.5",
            path { d: "M60 78c-16 0-26-12-26-28 0-8 3-15 7-19" }
            path { d: "M60 78c16 0 26-12 26-28 0-8-3-15-7-19" }
        }
        g { stroke: "var(--textDim)", stroke_width: "1.3", opacity: "0.7",
            path { d: "M41 33c-3-1-6 0-8 3 3 1 6 0 8-3z" }
            path { d: "M37 44c-3-1-6 0-8 3 3 1 6 0 8-3z" }
            path { d: "M37 56c-3 0-6 2-7 5 3 1 6-1 7-5z" }
            path { d: "M79 33c3-1 6 0 8 3-3 1-6 0-8-3z" }
            path { d: "M83 44c3-1 6 0 8 3-3 1-6 0-8-3z" }
            path { d: "M83 56c3 0 6 2 7 5-3 1-6-1-7-5z" }
        }
        path { d: "M52 50l6 6 12-12", stroke: "var(--accent)", stroke_width: "1.8" }
    })
}

/// Amphora — files.
pub fn IlloAmphora() -> Element {
    illo(rsx! {
        g { stroke: "var(--accent)", stroke_width: "1.6",
            path { d: "M50 24h20M53 24c0 7-10 10-10 20a17 17 0 0 0 34 0c0-10-10-13-10-20" }
            path { d: "M47 34c-6 0-10 3-10 7M73 34c6 0 10 3 10 7" }
            path { d: "M54 64h12l-2 8h-8z" }
        }
        g { stroke: "var(--textDim)", stroke_width: "1.2", opacity: "0.6",
            line { x1: "50", y1: "46", x2: "70", y2: "46" }
            line { x1: "49", y1: "52", x2: "71", y2: "52" }
        }
    })
}

/// Corinthian helmet — agents.
pub fn IlloHelmet() -> Element {
    illo(rsx! {
        g { stroke: "var(--accent)", stroke_width: "1.6",
            // dome + outer sides
            path { d: "M40 64 L40 56 C40 38 48 28 60 28 C72 28 80 38 80 56 L80 64" }
            // cheek guards turning inward (open mouth gap between)
            path { d: "M40 64 C40 69 44 72 49 72 L49 55" }
            path { d: "M80 64 C80 69 76 72 71 72 L71 55" }
            // nose guard (the vertical of the Corinthian 'T')
            line { x1: "60", y1: "44", x2: "60", y2: "67" }
            // eye openings
            path { d: "M50 47 q4 -3.2 7.5 0 q-3.75 2.6 -7.5 0 z" }
            path { d: "M62.5 47 q3.5 -3.2 7.5 0 q-3.75 2.6 -7.5 0 z" }
        }
        // crest / plume
        g { stroke: "var(--textDim)", stroke_width: "1.3", opacity: "0.75",
            path { d: "M48 27 C52 15 68 15 72 27" }
            line { x1: "53", y1: "20", x2: "55", y2: "26" }
            line { x1: "58", y1: "17", x2: "59", y2: "25" }
            line { x1: "63", y1: "17", x2: "63", y2: "25" }
            line { x1: "68", y1: "20", x2: "67", y2: "26" }
        }
    })
}
