//! Agent command utilities — ported from src/utils/agentCommands.ts
//!
//! Maps agent types to CLI commands, labels, and colors.

use crate::types::workspace::{AgentType, CustomAgent};

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

/// Build the resume command for an agent that supports session resumption.
///
/// Returns `None` for agents that do not support `--resume` (Custom, Shell).
/// The returned string has NO trailing newline — callers decide whether to
/// execute it (append `\n`) or merely display it.
pub fn get_agent_resume_command(agent_type: &AgentType, resume_id: &str) -> Option<String> {
    match agent_type {
        AgentType::Claude => Some(format!("claude --resume {}", resume_id)),
        AgentType::Codex => Some(format!("codex --resume {}", resume_id)),
        AgentType::Opencode => Some(format!("opencode --resume {}", resume_id)),
        AgentType::Gemini => Some(format!("gemini --resume {}", resume_id)),
        AgentType::Custom | AgentType::Shell => None,
    }
}

/// The foreground process name (as reported by `pty_agent_info`) for a given
/// agent type, used to detect whether the agent is already running in a pane.
/// Returns `None` for agent types that have no detectable long-running process.
pub fn agent_process_name(agent_type: &AgentType) -> Option<&'static str> {
    match agent_type {
        AgentType::Claude => Some("claude"),
        AgentType::Codex => Some("codex"),
        AgentType::Opencode => Some("opencode"),
        AgentType::Gemini => Some("gemini"),
        AgentType::Custom | AgentType::Shell => None,
    }
}

/// For a custom agent marked `is_claude`, the foreground process is still
/// `claude` (same binary, different flags), so running-detection can poll for
/// it. Returns `None` for custom agents that aren't Claude aliases.
pub fn custom_agent_process_name(is_claude: bool) -> Option<&'static str> {
    if is_claude {
        Some("claude")
    } else {
        None
    }
}

/// Build the set of resume commands available for a captured Claude session id.
///
/// Always includes the plain `claude --resume <id>`. For each custom agent
/// marked `is_claude`, also appends `<agent.command> --resume <id>` — the
/// agent's extra flags (e.g. `--model sonnet`) are preserved and `--resume
/// <id>` appended, since the user wants to choose which variant to resume
/// with. Deduped; returns at least one entry when called with a real id.
pub fn claude_resume_variants(resume_id: &str, claude_aliases: &[CustomAgent]) -> Vec<String> {
    let mut variants: Vec<String> = vec![format!("claude --resume {}", resume_id)];
    for agent in claude_aliases.iter().filter(|a| a.is_claude) {
        // The command already includes `claude` plus any extra flags; just
        // append `--resume <id>`. Strip a trailing `;` (the capture path adds
        // one) so we don't end up with `claude ...; --resume <id>`.
        let base = agent.command.trim().trim_end_matches(';').trim();
        if base.is_empty() {
            continue;
        }
        let variant = format!("{} --resume {}", base, resume_id);
        if !variants.contains(&variant) {
            variants.push(variant);
        }
    }
    variants
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::workspace::CustomAgent;

    const ID: &str = "2d63f514-75ac-4cca-96f4-0d78fa2941b3";

    fn agent(id: &str, alias: &str, command: &str, is_claude: bool) -> CustomAgent {
        CustomAgent {
            id: id.to_string(),
            alias: alias.to_string(),
            command: command.to_string(),
            is_claude,
        }
    }

    #[test]
    fn variants_always_include_plain_claude() {
        let v = claude_resume_variants(ID, &[]);
        assert_eq!(v, vec![format!("claude --resume {}", ID)]);
    }

    #[test]
    fn variants_include_is_claude_aliases() {
        let aliases = [
            agent("a1", "Sonnet", "claude --model sonnet", true),
            agent("a2", "not-claude", "codex", false),
        ];
        let v = claude_resume_variants(ID, &aliases);
        // Plain + the one is_claude alias; the codex one is excluded.
        assert_eq!(
            v,
            vec![
                format!("claude --resume {}", ID),
                format!("claude --model sonnet --resume {}", ID),
            ]
        );
    }

    #[test]
    fn variants_dedup_identical_commands() {
        // Two aliases with the exact same command should collapse.
        let aliases = [
            agent("a1", "A", "claude", true),
            agent("a2", "B", "claude", true),
        ];
        let v = claude_resume_variants(ID, &aliases);
        assert_eq!(v, vec![format!("claude --resume {}", ID)]);
    }

    #[test]
    fn variants_strip_trailing_semicolon() {
        // The capture path stores commands with a trailing ";"; the variant
        // builder must strip it so we don't emit `claude ...; --resume <id>`.
        let aliases = [agent("a1", "Sonnet", "claude --model sonnet;", true)];
        let v = claude_resume_variants(ID, &aliases);
        assert_eq!(
            v,
            vec![
                format!("claude --resume {}", ID),
                format!("claude --model sonnet --resume {}", ID),
            ]
        );
    }

    #[test]
    fn custom_agent_process_name_reflects_is_claude() {
        assert_eq!(custom_agent_process_name(true), Some("claude"));
        assert_eq!(custom_agent_process_name(false), None);
    }
}
