//! Shell-specific integration scripts and environment helpers.

use serde::{Deserialize, Serialize};

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
    // Single source of truth: the checked-in scripts under `shell/`. These
    // files are dual-purpose — injected into Athena PTY sessions via the
    // spawn-time startup mechanism AND sourceable manually from a user's own
    // rc file. The scripts' `__ATHENA_SOURCED` guards make double-sourcing a
    // no-op, which covers both paths.
    include_str!("../../../shell/athena-zsh.sh").to_string()
}

fn get_bash_integration() -> String {
    include_str!("../../../shell/athena-bash.bash").to_string()
}

fn get_fish_integration() -> String {
    include_str!("../../../shell/athena-fish.fish").to_string()
}
