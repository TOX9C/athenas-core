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

/// The subtle motif frame — two concentric rings that give each small
/// illustration a shared visual grammar. `stroke` is the dim color.
fn panel_frame(stroke: &str) -> Element {
    rsx! {
        // outer motif ring
        circle { cx: "60", cy: "48", r: "44", stroke: stroke, stroke_width: "1", fill: "none", stroke_opacity: "0.4" }
        // inner guide ring
        circle { cx: "60", cy: "48", r: "35", stroke: stroke, stroke_width: "1", fill: "none", stroke_opacity: "0.18" }
    }
}

/// A single gold sparkle at `(cx, cy)` — the accent focal mark. Each motif
/// places it at a different point so no two states share the same composition.
fn gold_sparkle(cx: &str, cy: &str) -> Element {
    let cx_f: f32 = cx.parse().unwrap_or(60.0);
    let cy_f: f32 = cy.parse().unwrap_or(48.0);
    let sparkle = format!(
        "M{cx_f:.2} {lo_y:.2} V{hi_y:.2} M{lo_x:.2} {cy_f:.2} H{hi_x:.2}",
        lo_y = cy_f - 2.4,
        hi_y = cy_f + 2.4,
        lo_x = cx_f - 2.4,
        hi_x = cx_f + 2.4,
    );
    rsx! {
        circle { cx: cx, cy: cy, r: "1.2", fill: "var(--accent)", stroke: "none" }
        path { d: "{sparkle}", stroke: "var(--accent)", stroke_width: "0.8", stroke_opacity: "0.6" }
    }
}

// ── Individual illustrations ────────────────────────────────────────────────

/// Window workspace — panes behind a shell prompt. Workspaces / generic.
pub(super) fn illo_workspace() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            // window frame
            rect { x: "34", y: "22", width: "52", height: "42", rx: "5" }
            // title bar line
            path { d: "M34 30 H86" }
            // grid splits
            path { d: "M60 30 V64 M34 46 H86" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.7",
            // shell prompt in the focused pane
            path { d: "M44 38 L50 43 L44 48" }
            path { d: "M54 48 H62" }
        }
        {gold_sparkle("92", "22")}
    })
}

/// Chat session — speech bubble with a sparkle. Sessions / history.
pub(super) fn illo_sessions() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            path { d: "M86 30 H58 A10 10 0 0 0 48 40 V56 A10 10 0 0 0 58 66 H66 L72 72 V66 H86 A10 10 0 0 0 96 56 V40 A10 10 0 0 0 86 30 Z" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.7",
            path { d: "M58 46 H80 M58 54 H72" }
        }
        {gold_sparkle("48", "40")}
    })
}

/// Kanban board — three columns with cards.
pub(super) fn illo_kanban() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            rect { x: "28", y: "24", width: "64", height: "44", rx: "5" }
            path { d: "M28 32 H92 M49.3 24 V68 M70.7 24 V68" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.7",
            // card in the center column
            rect { x: "55.5", y: "38", width: "9.5", height: "7", rx: "1.5" }
            path { d: "M55.5 50 H65 M55.5 56 H62" }
        }
        {gold_sparkle("92", "24")}
    })
}

/// Swarm network — hub with satellites.
pub(super) fn illo_swarm() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.3", opacity: "0.6",
            path { d: "M60 46 V28 M60 46 L82 34 M60 46 L82 60 M60 46 L40 60 M60 46 L38 34" }
        }
        g { stroke: "var(--textDim)", stroke_width: "1.5", fill: "var(--bg)",
            circle { cx: "60", cy: "28", r: "4" }
            circle { cx: "82", cy: "34", r: "4" }
            circle { cx: "82", cy: "60", r: "4" }
            circle { cx: "40", cy: "60", r: "4" }
            circle { cx: "38", cy: "34", r: "4" }
        }
        circle { cx: "60", cy: "46", r: "7", stroke: "var(--accent)", stroke_width: "1.8", fill: "var(--bg)" }
        circle { cx: "60", cy: "46", r: "2.4", fill: "var(--accent)", stroke: "none" }
        {gold_sparkle("82", "34")}
    })
}

/// Notifications — bell with a quiet dot.
pub(super) fn illo_notifications() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.6", opacity: "0.75",
            path { d: "M80 42 C80 28 72 24 60 24 C48 24 40 28 40 42 C40 56 36 58 36 58 H84 C84 58 80 56 80 42 Z" }
            path { d: "M68 68 C68 72 64 74 60 74 C56 74 52 72 52 68" }
        }
        circle { cx: "60", cy: "44", r: "2.2", fill: "var(--accent)", stroke: "none" }
        {gold_sparkle("86", "20")}
    })
}

/// Plugins — interlocking blocks.
pub(super) fn illo_plugins() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            path { d: "M52 28 H70 A6 6 0 0 1 76 34 V38 A4 4 0 0 1 72 42 V50 A4 4 0 0 1 76 54 V58 A6 6 0 0 1 70 64 H52 A6 6 0 0 1 46 58 V34 A6 6 0 0 1 52 28 Z" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.6",
            path { d: "M46 40 H42 A4 4 0 0 0 38 44 V54 A4 4 0 0 0 42 58 H46" }
            path { d: "M44 46 H40 M44 52 H41" }
        }
        {gold_sparkle("72", "34")}
    })
}

/// Files — folder with a document.
pub(super) fn illo_files() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            path { d: "M34 32 H56 L62 38 H88 A6 6 0 0 1 94 44 V64 A6 6 0 0 1 88 70 H40 A6 6 0 0 1 34 64 Z" }
        }
        g { stroke: "var(--accent)", stroke_width: "1.7",
            // document leaning on the folder
            path { d: "M56 34 V62 A4 4 0 0 0 60 66 H78 A4 4 0 0 0 82 62 V46 L72 34 Z" }
            path { d: "M72 34 V46 H82" }
            path { d: "M62 54 H76 M62 60 H72" }
        }
        {gold_sparkle("34", "30")}
    })
}

/// Agents — two avatars.
pub(super) fn illo_agents() -> Element {
    illo(rsx! {
        {panel_frame("var(--textDim)")}
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.75",
            path { d: "M38 72 V66 A8 8 0 0 1 46 58 H74 A8 8 0 0 1 82 66 V72" }
        }
        g { stroke: "var(--textDim)", stroke_width: "1.5", opacity: "0.6",
            circle { cx: "46", cy: "40", r: "8" }
            circle { cx: "74", cy: "40", r: "8" }
        }
        circle { cx: "46", cy: "40", r: "8", stroke: "var(--accent)", stroke_width: "1.8", fill: "var(--bg)" }
        {gold_sparkle("46", "40")}
    })
}
