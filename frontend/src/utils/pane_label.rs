//! Pure pane-label resolution for the title state machine.
//!
//! Extracted from `terminal_grid.rs` so the priority ladder is unit-testable
//! without rendering a Dioxus component. The raw prompt / scraped `task_title`
//! is NEVER used as a label here — that is the root fix for the
//! "whole prompt flashes as the title" bug. Only `TitleState::Done` (an LLM
//! title) or the static/random fallbacks are ever returned.

use crate::types::workspace::AgentType;
use crate::utils::pane_names::name_for_pane;

// Per-pane title state, owned by the frontend store. The backend owns the
// retry loop; the frontend only tracks whether a title is expected, in
// flight, failed, or available.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TitleState {
    /// No prompt scraped yet.
    #[default]
    Idle,
    /// Prompt scraped; the LLM call (with backend retries) is in flight.
    Pending,
    /// Backend exhausted retries or hit a non-retryable error.
    Failed,
    /// A title (or "Sensitive prompt") was produced.
    Done(String),
}

/// Resolve the visible left label for a pane. Priority:
///   1. user rename (`label`)
///   2. idle Shell → random name (only when `smart_on`)
///   3. agent pane → render `TitleState`
///   4. everything else → static agent label
///
/// `Pending` and `Failed` render an empty string (the raw prompt is never
/// shown). `Done` renders the title verbatim (truncation is view-only).
pub fn resolve_pane_label(
    label: Option<&str>,
    title_state: &TitleState,
    agent_type: &AgentType,
    fg_process: Option<&str>,
    smart_on: bool,
    static_agent_label: &str,
) -> String {
    // 1. User rename always wins.
    if let Some(l) = label {
        if !l.is_empty() {
            return l.to_string();
        }
    }

    let is_idle_shell =
        *agent_type == AgentType::Shell && fg_process.is_none_or(|p| p == "shell" || p.is_empty());

    // 2. Idle Shell → random name, only when smart titles are on.
    if is_idle_shell && smart_on {
        return name_for_pane(""); // pane_id not needed for determinism in tests
    }

    // 3. Agent pane → render TitleState.
    match title_state {
        TitleState::Done(title) => title.clone(),
        TitleState::Idle => static_agent_label.to_string(),
        // Pending / Failed → empty pill. The raw prompt is never shown.
        TitleState::Pending | TitleState::Failed => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> AgentType {
        AgentType::Shell
    }
    fn claude() -> AgentType {
        AgentType::Claude
    }

    #[test]
    fn user_rename_wins_regardless_of_state() {
        for state in [
            TitleState::Idle,
            TitleState::Pending,
            TitleState::Failed,
            TitleState::Done("x".into()),
        ] {
            assert_eq!(
                resolve_pane_label(
                    Some("my pane"),
                    &state,
                    &claude(),
                    None,
                    true,
                    "Claude Code"
                ),
                "my pane"
            );
        }
    }

    #[test]
    fn empty_rename_falls_through() {
        assert_eq!(
            resolve_pane_label(
                Some(""),
                &TitleState::Idle,
                &claude(),
                None,
                true,
                "Claude Code"
            ),
            "Claude Code"
        );
    }

    #[test]
    fn idle_agent_shows_static_label() {
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Idle,
                &claude(),
                None,
                true,
                "Claude Code"
            ),
            "Claude Code"
        );
    }

    #[test]
    fn pending_shows_empty() {
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Pending,
                &claude(),
                None,
                true,
                "Claude Code"
            ),
            ""
        );
    }

    #[test]
    fn failed_shows_empty() {
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Failed,
                &claude(),
                None,
                true,
                "Claude Code"
            ),
            ""
        );
    }

    #[test]
    fn done_shows_title() {
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Done("analyzing the codebase".into()),
                &claude(),
                None,
                true,
                "Claude Code"
            ),
            "analyzing the codebase"
        );
    }

    #[test]
    fn done_sensitive_prompt_shows_marker() {
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Done("Sensitive prompt".into()),
                &claude(),
                None,
                true,
                "Claude Code"
            ),
            "Sensitive prompt"
        );
    }

    #[test]
    fn idle_shell_smart_on_shows_random_name() {
        let name = resolve_pane_label(None, &TitleState::Idle, &shell(), None, true, "Shell");
        assert!(!name.is_empty());
        assert_ne!(name, "Shell");
    }

    #[test]
    fn idle_shell_smart_off_shows_static_label() {
        assert_eq!(
            resolve_pane_label(None, &TitleState::Idle, &shell(), None, false, "Shell"),
            "Shell"
        );
    }

    #[test]
    fn running_non_shell_process_falls_through_to_state() {
        // A shell running 'vim' is not "idle" — fg_process = "vim".
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Pending,
                &shell(),
                Some("vim"),
                true,
                "Shell"
            ),
            ""
        );
        assert_eq!(
            resolve_pane_label(
                None,
                &TitleState::Done("editing".into()),
                &shell(),
                Some("vim"),
                true,
                "Shell"
            ),
            "editing"
        );
    }
}
