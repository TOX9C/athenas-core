//! Shell-specific integration scripts and environment helpers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Shell integration scripts
// ---------------------------------------------------------------------------

/// Errors returned by `get_shell_integration_script`.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum ShellIntegrationError {
    /// The requested shell does not have a shell-integration script. Callers
    /// should treat this as a hard failure and not inject a fallback script,
    /// which may produce invalid syntax in the unsupported shell.
    #[error("unsupported shell for shell integration: {0} (supported: bash, zsh, fish)")]
    UnsupportedShell(String),
}

// ---------------------------------------------------------------------------
// Shell integration scripts
// ---------------------------------------------------------------------------

/// Return the shell integration script for the given shell.
///
/// Returns `Err(ShellIntegrationError::UnsupportedShell)` if the shell is
/// not one of `bash`, `zsh`, or `fish`. Callers MUST NOT inject a fallback
/// script for unknown shells — the script syntax is shell-specific and a
/// mismatched injection will break the target shell.
pub fn get_shell_integration_script(shell: &str) -> Result<String, ShellIntegrationError> {
    let base = shell.rsplit('/').next().unwrap_or("");

    match base {
        "zsh" => Ok(get_zsh_integration()),
        "bash" => Ok(get_bash_integration()),
        "fish" => Ok(get_fish_integration()),
        other => Err(ShellIntegrationError::UnsupportedShell(other.to_string())),
    }
}

fn get_zsh_integration() -> String {
    [
        "__athena_si_enabled=1",
        "",
        "__athena_osc633() { printf \"\\e]633;%s\\a\" \"$1\"; }",
        "",
        "__athena_precmd() {",
        "  local __athena_exit=$?",
        "  if [[ -n $__athena_si_last_cmd ]]; then",
        "    __athena_osc633 \"D;$__athena_exit\"",
        "    __athena_si_last_cmd=\"\"",
        "  fi",
        "  __athena_osc633 A",
        "  __athena_osc633 \"P;$PWD\"",
        "}",
        "",
        "__athena_preexec() {",
        "  __athena_si_last_cmd=\"$3\"",
        "  __athena_osc633 \"B;$3\"",
        "  __athena_osc633 C",
        "  __athena_osc633 E",
        "}",
        "",
        "autoload -Uz add-zsh-hook 2>/dev/null",
        "add-zsh-hook precmd __athena_precmd 2>/dev/null",
        "add-zsh-hook preexec __athena_preexec 2>/dev/null",
        "",
        "__athena_osc633 \"Set=shellIntegration=zsh\"",
    ]
    .join("\n")
}

fn get_bash_integration() -> String {
    [
        "__athena_si_enabled=1",
        "",
        "__athena_osc633() { printf \"\\e]633;%s\\a\" \"$1\"; }",
        "",
        "__athena_prompt_command() {",
        "  local __athena_exit=\"$?\"",
        "  if [[ -n $__athena_si_last_cmd ]]; then",
        "    __athena_osc633 \"D;$__athena_exit\"",
        "    __athena_si_last_cmd=\"\"",
        "  fi",
        "  __athena_osc633 A",
        "  __athena_osc633 \"P;$PWD\"",
        "}",
        "",
        "__athena_debug_trap() {",
        "  if [[ -n $__athena_si_last_cmd ]]; then",
        "    return",
        "  fi",
        "  local __athena_cmd=\"$BASH_COMMAND\"",
        "  if [[ \"$__athena_cmd\" != \"__athena_prompt_command\" && \"$__athena_cmd\" != *\"__athena_osc633\"* ]]; then",
        "    __athena_si_last_cmd=\"$__athena_cmd\"",
        "    __athena_osc633 \"B;$__athena_cmd\"",
        "    __athena_osc633 C",
        "    __athena_osc633 E",
        "  fi",
        "}",
        "",
        "trap \"__athena_debug_trap\" DEBUG",
        "PROMPT_COMMAND=\"__athena_prompt_command; $PROMPT_COMMAND\"",
        "",
        "__athena_osc633 \"Set=shellIntegration=bash\"",
    ]
    .join("\n")
}

fn get_fish_integration() -> String {
    [
        "set -g __athena_si_enabled 1",
        "",
        "function __athena_osc633 -d \"Emit OSC 633 sequence\"",
        "  printf \"\\e]633;%s\\a\" $argv",
        "end",
        "",
        "function __athena_prompt_start --on-event fish_prompt",
        "  __athena_osc633 A",
        "  __athena_osc633 \"P;(pwd)\"",
        "end",
        "",
        "function __athena_preexec --on-event fish_preexec",
        "  __athena_osc633 \"B;$argv\"",
        "  __athena_osc633 C",
        "  __athena_osc633 E",
        "end",
        "",
        "function __athena_postexec --on-event fish_postexec -a __athena_exit",
        "  __athena_osc633 \"D;$__athena_exit\"",
        "end",
        "",
        "__athena_osc633 \"Set=shellIntegration=fish\"",
    ]
    .join("\n")
}

/// Check whether the given shell is compatible with shell integration.
pub fn is_shell_integration_compatible(shell: &str) -> bool {
    if cfg!(windows) {
        return false;
    }
    let base = shell.rsplit('/').next().unwrap_or("");
    matches!(base, "zsh" | "bash" | "fish" | "sh")
}

/// Build environment variables for shell integration.
pub fn build_shell_integration_env(_shell: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("ATHENA_SHELL_INTEGRATION".to_string(), "1".to_string());
    map.insert("ATHENA_TERM".to_string(), "athena-core".to_string());
    map
}
