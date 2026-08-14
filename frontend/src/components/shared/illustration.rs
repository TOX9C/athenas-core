use dioxus::prelude::*;
#[path = "illustration_art.rs"]
mod illustration_art;

use illustration_art::{
    illo_agents, illo_files, illo_kanban, illo_notifications, illo_plugins, illo_sessions,
    illo_swarm, illo_workspace,
};

// ────────────────── Empty-state illustrations ──────────────────
// Inline SVG motifs inherit the active theme surface instead of introducing an
// opaque generated-image canvas into the empty state.

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

/// Theme-aware line-art illustration for an empty state.
///
/// The motifs use the active `--bg`, `--textDim`, and `--accent` tokens
/// directly, so they remain native to every theme and never expose a generated
/// image's background.
fn art_for(kind: EmptyArt) -> Element {
    match kind {
        EmptyArt::Workspace | EmptyArt::Generic => illo_workspace(),
        EmptyArt::Sessions => illo_sessions(),
        EmptyArt::Kanban => illo_kanban(),
        EmptyArt::Swarm => illo_swarm(),
        EmptyArt::Notifications => illo_notifications(),
        EmptyArt::Plugins => illo_plugins(),
        EmptyArt::Files => illo_files(),
        EmptyArt::Agents => illo_agents(),
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
            class: if kind == EmptyArt::Swarm { "empty-state swarm-empty-state" } else { "empty-state" },
            style: "flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; overflow: hidden;",
            div {
                class: "animate-rise",
                style: "display: flex; flex-direction: column; align-items: center; gap: 18px; padding: 32px; text-align: center;",
                div { class: "empty-state-art", aria_hidden: "true", {art_for(kind)} }
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

/// Brand mark for welcome / chat / thinking. `size` is in px.
/// A crystalline core: hexagonal frame, quiet inner ring, and a solid gold
/// diamond bezant. Pure geometry — no triangles — legible from 14px to 52px.
#[component]
pub fn CoreMark(size: Option<u16>) -> Element {
    let s = size.unwrap_or(20);
    let sz = format!("{s}px");
    const MARK_FRAME: &str = "M12 3.5 L19.36 7.75 L19.36 16.25 L12 20.5 L4.64 16.25 L4.64 7.75 Z";
    const MARK_RING: &str = "M12 6.6 L16.68 9.3 L16.68 14.7 L12 17.4 L7.32 14.7 L7.32 9.3 Z";
    const MARK_CORE: &str = "M12 9.3 L13.75 12 L12 14.7 L10.25 12 Z";
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            "aria-hidden": "true",
            stroke: "var(--accent)",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {sz}; height: {sz}; display: inline-block; vertical-align: middle;",
            path { d: "{MARK_FRAME}", fill: "var(--accent)", fill_opacity: "0.07", stroke: "var(--accent)", stroke_opacity: "0.85", stroke_width: "1.05" }
            path { d: "{MARK_RING}", stroke: "var(--accent)", stroke_opacity: "0.38", stroke_width: "0.5" }
            path { d: "{MARK_CORE}", fill: "var(--accent)", stroke: "none" }
        }
    }
}
