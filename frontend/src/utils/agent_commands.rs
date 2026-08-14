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
        AgentType::Qwen => Some("qwen-code".to_string()),
        AgentType::Aider => Some("aider".to_string()),
        AgentType::Cursor => Some("cursor-agent".to_string()),
        AgentType::Freebuff => Some("freebuff".to_string()),
        AgentType::Omp => Some("omp".to_string()),
        AgentType::Custom => custom_cmd.map(|s| s.to_string()),
        AgentType::Shell => None,
    }
}

/// Build the resume command for an agent that supports session resumption.
///
/// Returns `None` for agents that do not support `--resume` (Custom, Shell,
/// and the v1 detection-driven roster). The returned string has NO trailing
/// newline — callers decide whether to execute it (append `\n`) or merely
/// display it.
pub fn get_agent_resume_command(agent_type: &AgentType, resume_id: &str) -> Option<String> {
    match agent_type {
        AgentType::Claude => Some(format!("claude --resume {}", resume_id)),
        AgentType::Codex => Some(format!("codex --resume {}", resume_id)),
        AgentType::Opencode => Some(format!("opencode --resume {}", resume_id)),
        AgentType::Gemini => Some(format!("gemini --resume {}", resume_id)),
        AgentType::Qwen
        | AgentType::Aider
        | AgentType::Cursor
        | AgentType::Freebuff
        | AgentType::Omp
        | AgentType::Custom
        | AgentType::Shell => None,
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
        AgentType::Qwen => Some("qwen"),
        AgentType::Aider => Some("aider"),
        AgentType::Cursor => Some("cursor-agent"),
        AgentType::Freebuff => Some("freebuff"),
        AgentType::Omp => Some("omp"),
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
/// The priority agent (if any) is placed first so it appears as the default
/// selection in the dropdown. Then the plain `claude --resume <id>` is
/// included, followed by all other non-priority `is_claude` aliases. For each
/// custom agent marked `is_claude`, the command's extra flags (e.g. `--model
/// sonnet`) are preserved and `--resume <id>` appended. Deduped; returns at
/// least one entry when called with a real id.
pub fn claude_resume_variants(resume_id: &str, claude_aliases: &[CustomAgent]) -> Vec<String> {
    let mut variants: Vec<String> = vec![];

    // 1. Priority agent first (default selection)
    if let Some(agent) = claude_aliases.iter().find(|a| a.is_claude && a.priority) {
        let base = agent.command.trim().trim_end_matches(';').trim();
        if !base.is_empty() {
            variants.push(format!("{} --resume {}", base, resume_id));
        }
    }

    // 2. Plain claude ( deduplicated against priority if same )
    let plain = format!("claude --resume {}", resume_id);
    if !variants.contains(&plain) {
        variants.push(plain);
    }

    // 3. Remaining non-priority is_claude aliases
    for agent in claude_aliases.iter().filter(|a| a.is_claude && !a.priority) {
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
        AgentType::Qwen => "Qwen Code",
        AgentType::Aider => "Aider",
        AgentType::Cursor => "Cursor",
        AgentType::Freebuff => "Freebuff",
        AgentType::Omp => "OMP",
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
        AgentType::Qwen => "#6366f1",
        AgentType::Aider => "#eab308",
        AgentType::Cursor => "#22d3ee",
        AgentType::Freebuff => "#e11d48",
        AgentType::Omp => "#84cc16",
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
            priority: false,
        }
    }

    #[test]
    fn built_in_agent_commands_cover_omp_and_bypass() {
        assert_eq!(
            get_agent_command(&AgentType::Omp, None, false),
            Some("omp".to_string())
        );
        assert_eq!(
            get_agent_command(&AgentType::Claude, None, true),
            Some("claude --dangerously-skip-permissions".to_string())
        );
        assert_eq!(get_agent_command(&AgentType::Shell, None, false), None);
    }

    #[test]
    fn custom_agent_command_is_used_without_duplicate_launch() {
        assert_eq!(
            get_agent_command(&AgentType::Custom, Some("my-agent --interactive"), false),
            Some("my-agent --interactive".to_string())
        );
        assert_eq!(get_agent_command(&AgentType::Custom, None, false), None);
    }

    #[test]
    fn agent_process_names_include_omp_but_not_custom_shell() {
        assert_eq!(agent_process_name(&AgentType::Omp), Some("omp"));
        assert_eq!(agent_process_name(&AgentType::Custom), None);
        assert_eq!(agent_process_name(&AgentType::Shell), None);
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

    #[test]
    fn priority_agent_appears_first() {
        let aliases = [
            agent("a1", "Sonnet", "claude --model sonnet", true),
            agent("a2", "Opus", "claude --model opus", true),
        ];
        // Mark Sonnet as priority
        let mut aliases = aliases;
        aliases[0].priority = true;

        let v = claude_resume_variants(ID, &aliases);
        assert_eq!(v[0], format!("claude --model sonnet --resume {}", ID));
        assert_eq!(v[1], format!("claude --resume {}", ID));
        assert_eq!(v[2], format!("claude --model opus --resume {}", ID));
    }

    #[test]
    fn priority_agent_deduped_when_same_as_plain_claude() {
        // Plain claude command as priority — should not duplicate
        let aliases = [agent("a1", "Plain", "claude", true)];
        let mut aliases = aliases;
        aliases[0].priority = true;

        let v = claude_resume_variants(ID, &aliases);
        assert_eq!(v, vec![format!("claude --resume {}", ID)]);
    }

    #[test]
    fn non_priority_agents_appear_after_priority_and_plain() {
        let aliases = [
            agent("a1", "Sonnet", "claude --model sonnet", true), // not priority
            agent("a2", "Opus", "claude --model opus", true),     // priority
        ];
        let mut aliases = aliases;
        aliases[1].priority = true;

        let v = claude_resume_variants(ID, &aliases);
        assert_eq!(v[0], format!("claude --model opus --resume {}", ID));
        assert_eq!(v[1], format!("claude --resume {}", ID));
        assert_eq!(v[2], format!("claude --model sonnet --resume {}", ID));
    }

    #[test]
    fn no_priority_agent_falls_back_to_plain_claude_first() {
        let aliases = [
            agent("a1", "Sonnet", "claude --model sonnet", true),
            agent("a2", "Opus", "claude --model opus", true),
        ];
        let v = claude_resume_variants(ID, &aliases);
        assert_eq!(v[0], format!("claude --resume {}", ID));
        assert!(v.contains(&format!("claude --model sonnet --resume {}", ID)));
        assert!(v.contains(&format!("claude --model opus --resume {}", ID)));
    }
}
