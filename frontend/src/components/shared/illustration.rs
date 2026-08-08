use dioxus::prelude::*;
#[path = "illustration_art.rs"]
mod illustration_art;

use illustration_art::{
    illo_amphora, illo_constellation, illo_helmet, illo_laurel_wreath, illo_owl_branch,
    illo_scroll, illo_sleeping_owl, illo_temple,
};

// ────────────────── Empty-state line-art illustrations ──────────────────
// Larger, two-tone compositions in a black-figure / engraved style. Most
// strokes use --textDim; a single highlight stroke uses --accent so the art
// carries the gold identity. Rendered inside `EmptyState`.

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
        EmptyArt::Workspace | EmptyArt::Generic => illo_owl_branch(),
        EmptyArt::Sessions => illo_scroll(),
        EmptyArt::Kanban => illo_temple(),
        EmptyArt::Swarm => illo_constellation(),
        EmptyArt::Notifications => illo_sleeping_owl(),
        EmptyArt::Plugins => illo_laurel_wreath(),
        EmptyArt::Files => illo_amphora(),
        EmptyArt::Agents => illo_helmet(),
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
            // Outer container: flex centering, clips overflow; NO animation here.
            style: "flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; overflow: hidden;",
            // Inner wrapper: the actual entrance animation (avoids transform on the flex-stretching element that triggers parent scrollbar flash).
            div {
                class: "animate-rise",
                style: "display: flex; flex-direction: column; align-items: center; gap: 18px; padding: 32px; text-align: center;",
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
}

/// Brand owl mark — gold, for welcome / chat / thinking. `size` in px.
/// Drawn inside a faint engraved ring with the Θ halo (gold-leaf theta bar +
/// dot) behind the owl — the observatory's sighting disc. The `orbit-glow`
/// class is applied by the *caller* (lib.rs welcome plaque); this renders the
/// mark itself, ring-framed.
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
            // engraved outer sighting ring
            circle { cx: "12", cy: "12", r: "11", stroke: "var(--accent)", stroke_opacity: "0.35", stroke_width: "1" }
            // inner almucantar guide ring
            circle { cx: "12", cy: "12", r: "8.5", stroke: "var(--accent)", stroke_opacity: "0.2", stroke_width: "1" }
            // Θ halo — faint gold-leaf theta bar behind the owl
            path { d: "M3.5 12 H20.5", stroke: "var(--goldLeaf)", stroke_width: "1", stroke_opacity: "0.4" }
            // lapis degree-tick at the crown
            path { d: "M12 0.6 V2", stroke: "var(--accentLapis)", stroke_width: "1", stroke_opacity: "0.7" }
            // body / facial disc
            path { d: "M12 3.4c-4.2 0-7.2 3-7.2 7.2v2.4a7.2 7.2 0 0 0 14.4 0v-2.4c0-4.2-3-7.2-7.2-7.2z" }
            // brows that rise into ear tufts
            path { d: "M6.7 9.4c-.4-2.4.5-4.1 2-4.4 1.1.8 1.4 2.2 1.1 3.6" }
            path { d: "M17.3 9.4c.4-2.4-.5-4.1-2-4.4-1.1.8-1.4 2.2-1.1 3.6" }
            // eyes + pupils (pupils gold-leaf lit, matching the engraved Owl icon)
            circle { cx: "9", cy: "10.8", r: "2.4" }
            circle { cx: "15", cy: "10.8", r: "2.4" }
            circle { cx: "9", cy: "10.8", r: "0.85", fill: "var(--goldLeaf)", stroke: "none" }
            circle { cx: "15", cy: "10.8", r: "0.85", fill: "var(--goldLeaf)", stroke: "none" }
            // beak
            path { d: "M12 12.6l-1.1 1.8h2.2z" }
            // talons
            path { d: "M9.8 20.7v1.4M12 21.1v1.4M14.2 20.7v1.4" }
        }
    }
}
