//! SINGLE SOURCE OF TRUTH for agent-type and swarm-role identity colors and
//! human labels.
//!
//! Keys are the canonical lowercase agent-type strings ("claude", "codex",
//! ...) — the same strings the backend reports in the agent output store and
//! that `AgentType`'s strum `Display` serializes to (see
//! [`crate::types::workspace::AgentType`]). All other modules delegate here:
//! [`crate::utils::agent_commands`] wraps these for the enum-typed call sites,
//! and the swarm surfaces wrap the role helpers.

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
        "custom" => "#6b7280",
        "shell" => "#6b7280",
        _ => "var(--accent)",
    }
}

/// Get a color for a swarm role string ("coordinator", "builder", ...).
pub fn get_role_color_str(role: &str) -> &'static str {
    match role {
        "coordinator" => "#0ea5e9",
        "builder" => "#22c55e",
        "scout" => "#f59e0b",
        "reviewer" => "#06b6d4",
        _ => "var(--accentTeal)",
    }
}

/// Get a capitalized human label for a swarm role string.
pub fn get_role_label_str(role: &str) -> &'static str {
    match role {
        "coordinator" => "Coordinator",
        "builder" => "Builder",
        "scout" => "Scout",
        "reviewer" => "Reviewer",
        _ => "Agent",
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
        "custom" => "Custom",
        _ => "Agent",
    }
}

#[cfg(test)]
mod role_tests {
    use super::*;

    #[test]
    fn role_colors_cover_known_roles() {
        assert_eq!(get_role_color_str("coordinator"), "#0ea5e9");
        assert_eq!(get_role_color_str("builder"), "#22c55e");
        assert_eq!(get_role_color_str("scout"), "#f59e0b");
        assert_eq!(get_role_color_str("reviewer"), "#06b6d4");
    }

    #[test]
    fn role_colors_fallback_for_unknown() {
        assert_eq!(get_role_color_str("user"), "var(--accentTeal)");
    }

    #[test]
    fn role_labels_cover_known_roles() {
        assert_eq!(get_role_label_str("coordinator"), "Coordinator");
        assert_eq!(get_role_label_str("builder"), "Builder");
        assert_eq!(get_role_label_str("scout"), "Scout");
        assert_eq!(get_role_label_str("reviewer"), "Reviewer");
    }

    #[test]
    fn custom_agent_has_stable_gray() {
        // Custom agents must not fall back to the theme accent (gold); they
        // get the fixed gray that the old AgentType-keyed palette used.
        assert_eq!(get_agent_color_str("custom"), "#6b7280");
    }
}

/// Best-effort human label for an agent surface. Prefers the human name of
/// the agent's type; falls back to the pane id when the type is unknown or
/// empty so surfaces never render a bare uuid as the primary identity.
pub fn get_agent_display_name(agent_type: &str, pane_id: &str) -> String {
    if agent_type.is_empty() {
        return pane_id.to_string();
    }
    let label = get_agent_label_str(agent_type);
    if label == "Agent" {
        pane_id.to_string()
    } else {
        label.to_string()
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
        // "custom" is a first-class agent type with its own gray.
        assert_eq!(get_agent_color_str("custom"), "#6b7280");
        assert_eq!(get_agent_color_str(""), "var(--accent)");
        assert_eq!(get_agent_color_str("unknown-type"), "var(--accent)");
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
        assert_eq!(get_agent_label_str("custom"), "Custom");
    }

    #[test]
    fn display_name_prefers_type_label_and_falls_back_to_pane() {
        assert_eq!(get_agent_display_name("claude", "pane-1"), "Claude");
        assert_eq!(get_agent_display_name("shell", "pane-2"), "Shell");
        assert_eq!(get_agent_display_name("custom", "pane-3"), "Custom");
        // Unknown or empty types fall back to the raw pane id so users
        // never see a bare uuid or a generic "Agent" as the primary name.
        assert_eq!(get_agent_display_name("unknown-type", "pane-9"), "pane-9");
        assert_eq!(get_agent_display_name("", "pane-8"), "pane-8");
    }

    #[test]
    fn labels_fallback_for_unknown() {
        // "custom" is a first-class agent type with its own label.
        assert_eq!(get_agent_label_str("custom"), "Custom");
        assert_eq!(get_agent_label_str(""), "Agent");
        assert_eq!(get_agent_label_str("unknown-type"), "Agent");
    }
}
