//! Agent command utilities — ported from src/utils/agentCommands.ts
//!
//! Maps agent types to CLI commands, labels, and colors.

use crate::types::workspace::AgentType;

/// **SECURITY WARNING**: This flag bypasses all Claude Code permission checks.
/// Only use this in trusted/development environments. Never use in production.
const CLAUDE_SKIP_PERMISSIONS_FLAG: &str = "--dangerously-skip-permissions";

/// Get the CLI command for an agent type.
pub fn get_agent_command(
    agent_type: &AgentType,
    custom_cmd: Option<&str>,
    bypass: bool,
) -> Option<String> {
    match agent_type {
        AgentType::Claude => {
            if bypass {
                Some(format!("claude {}", CLAUDE_SKIP_PERMISSIONS_FLAG))
            } else {
                Some("claude".to_string())
            }
        }
        AgentType::Codex => Some("codex".to_string()),
        AgentType::Opencode => Some("opencode".to_string()),
        AgentType::Gemini => Some("gemini".to_string()),
        AgentType::Custom => custom_cmd.map(|s| s.to_string()),
        AgentType::Shell => None,
    }
}

/// Get the human-readable label for an agent type.
pub fn get_agent_label(agent_type: &AgentType) -> &'static str {
    match agent_type {
        AgentType::Claude => "Claude Code",
        AgentType::Codex => "Codex",
        AgentType::Opencode => "OpenCode",
        AgentType::Gemini => "Gemini CLI",
        AgentType::Custom => "Custom",
        AgentType::Shell => "Shell",
    }
}

/// Get the accent color for an agent type.
pub fn get_agent_color(agent_type: &AgentType) -> &'static str {
    match agent_type {
        AgentType::Claude => "#d97706",
        AgentType::Codex => "#10b981",
        AgentType::Opencode => "#3b82f6",
        AgentType::Gemini => "#0891b2",
        AgentType::Custom => "#6b7280",
        AgentType::Shell => "#64748b",
    }
}
