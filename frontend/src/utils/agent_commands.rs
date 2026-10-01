//! Agent command utilities — ported from src/utils/agentCommands.ts
//!
//! Maps agent types to CLI commands. Labels and colors are NOT defined here:
//! they delegate to [`crate::utils::agent_display`], the single source of
//! truth (this module used to carry a second, divergent palette).

use crate::types::workspace::{AgentType, CustomAgent};
use crate::utils::agent_display::{get_agent_color_str, get_agent_label_str};

/// **SECURITY WARNING**: This flag bypasses all Claude Code permission checks.
/// Only use this in trusted/development environments. Never use in production.
const CLAUDE_SKIP_PERMISSIONS_FLAG: &str = "--dangerously-skip-permissions";

/// Get the CLI command for an agent type.
/// Quote one literal as a single-quoted POSIX shell argument.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

pub fn get_agent_command(
    agent_type: &AgentType,
    custom_cmd: Option<&str>,
    bypass: bool,
    model: Option<&str>,
) -> Option<String> {
    // The model string travels to the CLI verbatim; force-quote it.
    let model_flag = model
        .filter(|m| !m.trim().is_empty())
        .map(|m| format!(" --model {}", shq(m.trim())))
        .unwrap_or_default();
    match agent_type {
        AgentType::Claude => {
            if bypass {
                Some(format!("claude {}{}", CLAUDE_SKIP_PERMISSIONS_FLAG, model_flag))
            } else {
                Some(format!("claude{}", model_flag))
            }
        }
        AgentType::Codex => Some(format!("codex{}", model_flag)),
        AgentType::Opencode => Some(format!("opencode{}", model_flag)),
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

/// Build the resume/continuation command for an agent that supports session
/// restoration.
///
/// Returns `None` for agents whose resume protocol is not known (Qwen, Aider,
/// Cursor, Custom, and Shell). The returned string has NO trailing newline —
/// callers decide whether to execute it (append `\n`) or merely display it.
pub fn get_agent_resume_command(agent_type: &AgentType, resume_id: &str) -> Option<String> {
    match agent_type {
        AgentType::Claude => Some(format!("claude --resume {}", resume_id)),
        AgentType::Codex => Some(format!("codex --resume {}", resume_id)),
        AgentType::Opencode => Some(format!("opencode --resume {}", resume_id)),
        AgentType::Gemini => Some(format!("gemini --resume {}", resume_id)),
        AgentType::Freebuff => Some(format!("freebuff --continue {}", resume_id)),
        AgentType::Omp => Some(format!("omp --resume {}", resume_id)),
        AgentType::Qwen
        | AgentType::Aider
        | AgentType::Cursor
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
///
/// `AgentType`'s strum `Display` serializes to the canonical lowercase keys
/// ("claude", "codex", ...), so this just forwards to the shared table in
/// [`crate::utils::agent_display`].
pub fn get_agent_label(agent_type: &AgentType) -> &'static str {
    get_agent_label_str(&agent_type.to_string())
}

/// Get the accent color for an agent type.
///
/// See [`get_agent_label`] for the delegation note.
pub fn get_agent_color(agent_type: &AgentType) -> &'static str {
    get_agent_color_str(&agent_type.to_string())
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
            get_agent_command(&AgentType::Omp, None, false, None),
            Some("omp".to_string())
        );
        assert_eq!(
            get_agent_command(&AgentType::Claude, None, true, None),
            Some("claude --dangerously-skip-permissions".to_string())
        );
        assert_eq!(get_agent_command(&AgentType::Shell, None, false, None), None);
    }

    #[test]
    fn custom_agent_command_is_used_without_duplicate_launch() {
        assert_eq!(
            get_agent_command(&AgentType::Custom, Some("my-agent --interactive"), false, None),
            Some("my-agent --interactive".to_string())
        );
        assert_eq!(get_agent_command(&AgentType::Custom, None, false, None), None);
    }

    #[test]
    fn resume_commands_cover_freebuff_and_omp() {
        assert_eq!(
            get_agent_resume_command(&AgentType::Freebuff, "2026-08-15T11-30-56.357Z"),
            Some("freebuff --continue 2026-08-15T11-30-56.357Z".to_string())
        );
        assert_eq!(
            get_agent_resume_command(&AgentType::Omp, "019ff77f-fadb-7000-b51d-b7b38c9cb0eb"),
            Some("omp --resume 019ff77f-fadb-7000-b51d-b7b38c9cb0eb".to_string())
        );
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

/// Build the spawn command for a scoped pane: prepend proxy/MCP env-strip
/// when disallowed, wrap the agent in `sandbox-exec` (macOS-only; this app
/// targets macOS) for the file/network restrictions.
///
/// If `sandbox-exec` is genuinely missing the spawn fails loudly, which is
/// correct for a security boundary (no silent partial protection).
pub fn wrap_with_capabilities(
    cmd: String,
    caps: Option<&crate::types::workspace::RoleCapabilities>,
    cwd: &str,
) -> String {
    use crate::types::workspace::ShellPolicy;
    let Some(c) = caps else { return cmd };
    if c.shell == ShellPolicy::Full && c.network && c.mcp_tools {
        return cmd;
    }

    // 1) env strip for proxy + MCP plumbing, best-effort.
    let mut env_prefix = String::new();
    if !c.network {
        env_prefix.push_str(
            "env -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u http_proxy -u https_proxy -u all_proxy ",
        );
    }
    if !c.mcp_tools {
        env_prefix.push_str("ATHENA_NO_MCP=1 MCP_CONFIG_PATH=/dev/null ");
    }

    // 2) sandbox profile for fs/network restrictions.
    let need_sandbox = c.shell != ShellPolicy::Full || !c.network;
    if !need_sandbox {
        return format!("{env_prefix}{cmd}");
    }
    let mut profile = String::from("(version 1) (allow default)");
    if c.shell != ShellPolicy::Full {
        // Deny writes under the agent's cwd (the read-only contract:
        // agents may read/exec, never mutate the tree).
        let escaped_cwd = cwd.replace('"', "\\\"");
        profile.push_str(&format!(
            " (deny file-write* (subpath \"{escaped_cwd}\"))"
        ));
        if c.shell == ShellPolicy::None {
            profile.push_str(" (deny process-exec process-fork)");
        }
    }
    if !c.network {
        profile.push_str(" (deny network*)");
    }
    let inner = format!("{}sh -c {}", env_prefix, sh_quote(&cmd));
    format!("sandbox-exec -p {} {}", sh_quote(&profile), inner)
}

/// POSIX single-quoted literal for strings inside a generated `sh -c`.
fn sh_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.'))
    {
        return s.to_string();
    }
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

#[cfg(test)]
mod wrap_tests {
    use super::*;
    use crate::types::workspace::{RoleCapabilities, ShellPolicy};

    #[test]
    fn default_caps_pass_through() {
        let caps = RoleCapabilities::default();
        assert_eq!(wrap_with_capabilities("claude".into(), Some(&caps), "/x"), "claude");
        assert_eq!(wrap_with_capabilities("claude".into(), None, "/x"), "claude");
    }

    #[test]
    fn network_off_strips_proxies_and_denies_network() {
        let caps = RoleCapabilities { network: false, ..Default::default() };
        let out = wrap_with_capabilities("claude".into(), Some(&caps), "/x");
        assert!(out.contains("env -u HTTP_PROXY"), "{out}");
        assert!(out.contains("deny network"), "{out}");
        assert!(out.contains("sandbox-exec"), "{out}");
    }

    #[test]
    fn readonly_denies_cwd_writes() {
        let caps = RoleCapabilities { shell: ShellPolicy::ReadOnly, ..Default::default() };
        let out = wrap_with_capabilities("claude".into(), Some(&caps), "/repo");
        assert!(out.contains("file-write*"), "{out}");
        assert!(out.contains("/repo"), "{out}");
        assert!(!out.contains("network"), "{out}");
    }

    #[test]
    fn none_mode_drops_exec_and_mcp() {
        let caps = RoleCapabilities { shell: ShellPolicy::None, network: false, mcp_tools: false };
        let out = wrap_with_capabilities("claude".into(), Some(&caps), "/r");
        assert!(out.contains("process-exec"), "{out}");
        assert!(out.contains("ATHENA_NO_MCP=1"), "{out}");
    }

    #[test]
    fn quoting_preserves_inner_command() {
        let caps = RoleCapabilities { shell: ShellPolicy::ReadOnly, ..Default::default() };
        let out = wrap_with_capabilities("codex --model x".into(), Some(&caps), "/r");
        assert!(out.contains("sh -c"), "{out}");
        assert!(out.contains("'codex --model x'"), "{out}");
    }
}
