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

/// The Athena wave: the squiggle from the app icon as a single stroked line.
/// Legible from 14px to 52px. Shared with icon_mythology.rs and
/// icons/athena.svg.
#[component]
pub fn CoreMark(size: Option<u16>) -> Element {
    let s = size.unwrap_or(20);
    let sz = format!("{s}px");
    const MARK_WAVE: &str =
        "M2.5 17 C3.5 12, 5 9.5, 7 10 C9.5 10.5, 10 15, 13 13.5 C16 12, 16.5 6.5, 19 8 C20.7 9.2, 21.8 11.5, 21.5 14";
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            "aria-hidden": "true",
            stroke: "var(--accent)",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            style: "width: {sz}; height: {sz}; display: inline-block; vertical-align: middle;",
            path { d: "{MARK_WAVE}", fill: "none", stroke: "var(--accent)", stroke_width: 3.4 }
        }
    }
}
