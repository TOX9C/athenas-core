use dioxus::prelude::*;

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

/// The astrolabe ring frame — three concentric engraved rings + 12 degree ticks.
/// Each empty-state illo carries its own frame so the art reads as a single
/// instrument observation. Drawn on the same 120×96 viewBox as `illo`, fitted
/// to the lower 96px-tall plaque (so the rings sit centered behind the motif).
/// `stroke` is the dim engraved color (callers pass `var(--textDim)`).
fn astrolabe_frame(stroke: &str) -> Element {
    rsx! {
        // outer ring (engraved disc rim)
        circle { cx: "60", cy: "48", r: "45", stroke: stroke, stroke_width: "1", fill: "none", stroke_opacity: "0.55" }
        // mid ring (almucantar guide)
        circle { cx: "60", cy: "48", r: "37", stroke: stroke, stroke_width: "1", fill: "none", stroke_opacity: "0.28" }
        // inner ring (the ecliptica)
        circle { cx: "60", cy: "48", r: "29", stroke: stroke, stroke_width: "1", fill: "none", stroke_opacity: "0.18" }
        // 12 degree ticks around the outer ring (cardinal + intercardinal + thirds)
        g { stroke: stroke, stroke_width: "1", stroke_opacity: "0.5",
            // cardinals
            path { d: "M60 3 V10" }
            path { d: "M60 86 V93" }
            path { d: "M15 48 H22" }
            path { d: "M98 48 H105" }
            // diagonals
            path { d: "M28 16 L33 21" }
            path { d: "M87 16 L82 21" }
            path { d: "M28 80 L33 75" }
            path { d: "M87 80 L82 75" }
            // tertiary ticks
            path { d: "M49 4.4 L50.5 11" }
            path { d: "M71 4.4 L69.5 11" }
            path { d: "M49 91.6 L50.5 85" }
            path { d: "M71 91.6 L69.5 85" }
        }
        // lapis star — one per illo, placed by caller via a separate element.
    }
}

/// A single lapis star at `(cx, cy)` — the instrument's sighting mark. Each illo
/// places it at a different point inside the frame so no two panels share a
/// star (the constellation varies across the observatory).
fn lapis_star(cx: &str, cy: &str) -> Element {
    let cx_f: f32 = cx.parse().unwrap_or(60.0);
    let cy_f: f32 = cy.parse().unwrap_or(48.0);
    // 4-point sparkle cross, length 3.6px from center
    let sparkle = format!(
        "M{cx_f:.2} {lo_y:.2} V{hi_y:.2} M{lo_x:.2} {cy_f:.2} H{hi_x:.2}",
        lo_y = cy_f - 1.8,
        hi_y = cy_f + 1.8,
        lo_x = cx_f - 1.8,
        hi_x = cx_f + 1.8,
    );
    rsx! {
        circle { cx: cx, cy: cy, r: "1.1", fill: "var(--accentLapis)", stroke: "none", opacity: "0.95" }
        // faint four-point sparkle cross
        path { d: "{sparkle}", stroke: "var(--accentLapis)", stroke_width: "0.6", stroke_opacity: "0.55" }
    }
}

// ── Individual illustrations ────────────────────────────────────────────────

/// Owl perched on a branch — workspaces / generic. Astrolabe-framed, lapis
/// star sights the upper-left of the disc.
pub(super) fn illo_owl_branch() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("28", "20")}
    })
}

/// Unrolled scroll — sessions / history. Astrolabe-framed, lapis star sights
/// the upper-right.
pub(super) fn illo_scroll() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("92", "22")}
    })
}

/// Temple façade columns — kanban board. Astrolabe-framed, lapis star sights
/// the apex of the pediment (over the gable).
pub(super) fn illo_temple() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("60", "10")}
    })
}

/// Constellation network — swarm. Astrolabe-framed; each satellite joins the
/// outer ring's degree ticks, the lapis star sights the far-right node.
pub(super) fn illo_constellation() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("88", "36")}
    })
}

/// Sleeping owl — no notifications. Astrolabe-framed, lapis star sights
/// the upper-left where the zzz drifts.
pub(super) fn illo_sleeping_owl() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("96", "16")}
    })
}

/// Laurel wreath — plugins. Astrolabe-framed, lapis star sights the center
/// check (the laureate's seal).
pub(super) fn illo_laurel_wreath() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("60", "48")}
    })
}

/// Amphora — files. Astrolabe-framed, lapis star sights the upper-right
/// (the vessel's open handle).
pub(super) fn illo_amphora() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
        g { stroke: "var(--accent)", stroke_width: "1.6",
            path { d: "M50 24h20M53 24c0 7-10 10-10 20a17 17 0 0 0 34 0c0-10-10-13-10-20" }
            path { d: "M47 34c-6 0-10 3-10 7M73 34c6 0 10 3 10 7" }
            path { d: "M54 64h12l-2 8h-8z" }
        }
        g { stroke: "var(--textDim)", stroke_width: "1.2", opacity: "0.6",
            line { x1: "50", y1: "46", x2: "70", y2: "46" }
            line { x1: "49", y1: "52", x2: "71", y2: "52" }
        }
        {lapis_star("90", "26")}
    })
}

/// Corinthian helmet — agents. Astrolabe-framed, lapis star sights the
/// upper-left where the crest plume rises.
pub(super) fn illo_helmet() -> Element {
    illo(rsx! {
        {astrolabe_frame("var(--textDim)")}
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
        {lapis_star("30", "14")}
    })
}
