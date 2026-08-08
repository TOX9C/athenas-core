//! Agent display helpers keyed by the raw agent-type string reported by the
//! agent output store.
//!
//! These intentionally mirror the *legacy* agent-type strings ("claude",
//! "codex", ...) rather than the `AgentType` enum in
//! [`crate::utils::agent_commands`], whose palette and labels differ. Both
//! surfaces are kept so existing rendered colors/labels stay unchanged.

/// Get a color for an agent-type string.
pub fn get_agent_color_str(agent_type: &str) -> &'static str {
    match agent_type {
        "claude" => "#f97316",
        "codex" => "#10b981",
        "opencode" => "#8b5cf6",
        "gemini" => "#3b82f6",
        "qwen" => "#6366f1",
        "aider" => "#eab308",
        "cursor" => "#22d3ee",
        "freebuff" => "#e11d48",
        "omp" => "#84cc16",
        "shell" => "#6b7280",
        _ => "var(--accent)",
    }
}

/// Get a label for an agent-type string.
pub fn get_agent_label_str(agent_type: &str) -> &'static str {
    match agent_type {
        "claude" => "Claude",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "gemini" => "Gemini",
        "qwen" => "Qwen Code",
        "aider" => "Aider",
        "cursor" => "Cursor",
        "freebuff" => "Freebuff",
        "omp" => "OMP",
        "shell" => "Shell",
        _ => "Agent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_cover_known_types() {
        assert_eq!(get_agent_color_str("claude"), "#f97316");
        assert_eq!(get_agent_color_str("codex"), "#10b981");
        assert_eq!(get_agent_color_str("opencode"), "#8b5cf6");
        assert_eq!(get_agent_color_str("gemini"), "#3b82f6");
        assert_eq!(get_agent_color_str("qwen"), "#6366f1");
        assert_eq!(get_agent_color_str("aider"), "#eab308");
        assert_eq!(get_agent_color_str("cursor"), "#22d3ee");
        assert_eq!(get_agent_color_str("freebuff"), "#e11d48");
        assert_eq!(get_agent_color_str("omp"), "#84cc16");
        assert_eq!(get_agent_color_str("shell"), "#6b7280");
    }

    #[test]
    fn colors_fallback_to_accent_for_unknown() {
        assert_eq!(get_agent_color_str("custom"), "var(--accent)");
        assert_eq!(get_agent_color_str(""), "var(--accent)");
    }

    #[test]
    fn labels_cover_known_types() {
        assert_eq!(get_agent_label_str("claude"), "Claude");
        assert_eq!(get_agent_label_str("codex"), "Codex");
        assert_eq!(get_agent_label_str("opencode"), "OpenCode");
        assert_eq!(get_agent_label_str("gemini"), "Gemini");
        assert_eq!(get_agent_label_str("qwen"), "Qwen Code");
        assert_eq!(get_agent_label_str("aider"), "Aider");
        assert_eq!(get_agent_label_str("cursor"), "Cursor");
        assert_eq!(get_agent_label_str("freebuff"), "Freebuff");
        assert_eq!(get_agent_label_str("omp"), "OMP");
        assert_eq!(get_agent_label_str("shell"), "Shell");
    }

    #[test]
    fn labels_fallback_for_unknown() {
        assert_eq!(get_agent_label_str("custom"), "Agent");
        assert_eq!(get_agent_label_str(""), "Agent");
    }
}
