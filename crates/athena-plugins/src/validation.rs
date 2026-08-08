//! Security validation for plugin manifests and install commands.

use std::collections::HashMap;
use std::path::{Component, Path};

use super::{PluginError, PluginInstallMethod, PluginManifest};

// ---------------------------------------------------------------------------
// Manifest validation
// ---------------------------------------------------------------------------

/// Allowed executable names for MCP server commands.
const ALLOWED_MCP_COMMANDS: &[&str] = &[
    "node", "python", "python3", "ruby", "cargo", "sh", "bash", "zsh", "npx", "deno", "uv", "uvx",
    "pipx",
];

/// Shell metacharacters that indicate injection risk in hook scripts.
const SHELL_METACHARACTERS: &[char] = &[';', '|', '&', '$', '`', '\n'];

/// Validate a plugin manifest before registration.
///
/// Checks `install` and `mcp_config` fields for unsafe values that could
/// lead to arbitrary code execution:
///
/// - **Hook scripts**: must be simple relative paths (no metacharacters,
///   no absolute paths, no path traversal).
/// - **MCP commands**: must be a whitelisted executable name (no absolute
///   paths, no `./` prefixes).
/// - **MCP env**: must not override `PATH` or `HOME`.
pub fn validate_plugin_manifest(manifest: &PluginManifest) -> Result<(), PluginError> {
    // Validate the install method if present.
    if let Some(ref install) = manifest.install {
        validate_plugin_install_method(install)?;
    }

    // Validate the embedded mcp_config if present.
    if let Some(ref mcp_config) = manifest.mcp_config {
        validate_mcp_command(&mcp_config.command)?;
        if let Some(ref env) = mcp_config.env {
            validate_mcp_env(env)?;
        }
    }

    Ok(())
}

/// Validate a [`PluginInstallMethod`].
pub fn validate_plugin_install_method(method: &PluginInstallMethod) -> Result<(), PluginError> {
    match method {
        PluginInstallMethod::Builtin => Ok(()),
        PluginInstallMethod::McpServer {
            command,
            args: _,
            env,
        } => {
            validate_mcp_command(command)?;
            if let Some(ref env_map) = env {
                validate_mcp_env(env_map)?;
            }
            Ok(())
        }
        PluginInstallMethod::Hook { script } => validate_hook_script(script),
    }
}

fn validate_hook_script(script: &str) -> Result<(), PluginError> {
    let path = Path::new(script);

    // Reject absolute paths (has root component or drive letter on Windows).
    if path.is_absolute() || path.has_root() {
        return Err(PluginError::ValidationFailed(format!(
            "hook script must be a relative path, got absolute: {script}"
        )));
    }

    // Reject path traversal (.. in any form, including Windows ..\).
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(PluginError::ValidationFailed(format!(
            "hook script must not traverse parent directories: {script}"
        )));
    }

    // Reject shell metacharacters.
    if let Some(pos) = script
        .chars()
        .position(|c| SHELL_METACHARACTERS.contains(&c))
    {
        let ch = script.chars().nth(pos).unwrap_or('?');
        return Err(PluginError::ValidationFailed(format!(
            "hook script contains shell metacharacter '{}': {script}",
            ch
        )));
    }

    Ok(())
}

fn validate_mcp_command(command: &str) -> Result<(), PluginError> {
    // Reject absolute paths.
    if command.starts_with('/') {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command must be a bare executable name, got absolute path: {command}"
        )));
    }

    // Reject relative path prefixes (e.g. "./malicious").
    if command.starts_with("./") || command.starts_with("../") {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command must be a bare executable name, got relative path: {command}"
        )));
    }

    // Reject if it contains a directory separator (e.g. "bin/node").
    if command.contains('/') {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command must be a bare executable name, got path with '/': {command}"
        )));
    }

    // Must be on the whitelist.
    if !ALLOWED_MCP_COMMANDS.contains(&command) {
        return Err(PluginError::ValidationFailed(format!(
            "MCP command '{}' is not allowed. Permitted commands: {}",
            command,
            ALLOWED_MCP_COMMANDS.join(", ")
        )));
    }

    Ok(())
}

fn validate_mcp_env(env: &HashMap<String, String>) -> Result<(), PluginError> {
    let forbidden = ["PATH", "HOME"];
    for key in env.keys() {
        if forbidden.contains(&key.as_str()) {
            return Err(PluginError::ValidationFailed(format!(
                "MCP env must not override '{key}'"
            )));
        }
    }
    Ok(())
}
