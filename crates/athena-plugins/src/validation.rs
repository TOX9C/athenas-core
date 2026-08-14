//! Security validation for plugin manifests and install commands.

use std::collections::HashMap;
use std::path::{Component, Path};

use super::{
    PluginError, PluginInstallMethod, PluginManifest, MAX_PLUGIN_CONFIG_BYTES,
    MAX_PLUGIN_EVENT_BYTES,
};
use serde_json::Value;

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
    validate_identifier("plugin id", &manifest.id, 128)?;
    validate_text("plugin name", &manifest.name, 256)?;
    validate_text("plugin description", &manifest.description, 8 * 1024)?;
    validate_text("plugin author", &manifest.author, 256)?;
    if let Some(subscriptions) = &manifest.subscribes_to {
        if subscriptions.len() > 32 {
            return Err(PluginError::LimitExceeded(
                "plugin declares too many event subscriptions".to_string(),
            ));
        }
    }
    if let Some(config) = &manifest.config {
        let bytes = serde_json::to_vec(config).map_err(|e| {
            PluginError::ValidationFailed(format!("plugin config schema is not serializable: {e}"))
        })?;
        if bytes.len() > MAX_PLUGIN_CONFIG_BYTES {
            return Err(PluginError::LimitExceeded(
                "plugin config schema exceeds 256 KiB".to_string(),
            ));
        }
        validate_schema_definition(&config.schema, "$".to_string(), 0)?;
        validate_plugin_config(&config.schema, &config.defaults)?;
    }
    if let Some(subscriptions) = &manifest.subscribes_to {
        let bytes = serde_json::to_vec(subscriptions).map_err(|e| {
            PluginError::ValidationFailed(format!("plugin subscriptions are not serializable: {e}"))
        })?;
        if bytes.len() > MAX_PLUGIN_EVENT_BYTES {
            return Err(PluginError::LimitExceeded(
                "plugin subscription declaration exceeds 256 KiB".to_string(),
            ));
        }
    }

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

/// Validate plugin configuration against the manifest's JSON-Schema subset.
///
/// The host accepts a deliberately bounded JSON-Schema subset. Unsupported
/// validation keywords and malformed schema values are rejected at manifest
/// registration time rather than silently ignored.
pub fn validate_plugin_config(schema: &Value, config: &Value) -> Result<(), PluginError> {
    validate_schema_definition(schema, "$".to_string(), 0)?;
    validate_schema_node(schema, config, "$".to_string(), 0)
}

const MAX_SCHEMA_DEPTH: usize = 64;

fn validate_schema_definition(
    schema: &Value,
    path: String,
    depth: usize,
) -> Result<(), PluginError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(PluginError::LimitExceeded(format!(
            "plugin config schema exceeds maximum nesting depth at {path}"
        )));
    }
    if schema.is_boolean() {
        return Ok(());
    }
    let Some(schema) = schema.as_object() else {
        return Err(PluginError::ValidationFailed(format!(
            "plugin configuration schema must be an object or boolean at {path}"
        )));
    };

    // Only annotation keywords outside the supported validation subset are
    // tolerated. Rejecting unknown keywords keeps a plugin from appearing to
    // have stronger validation than the host actually performs.
    const ALLOWED_ANNOTATIONS: &[&str] = &[
        "$comment",
        "$id",
        "$schema",
        "title",
        "description",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
    ];
    const SUPPORTED_KEYWORDS: &[&str] = &[
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
        "const",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
    ];
    for keyword in schema.keys() {
        if !SUPPORTED_KEYWORDS.contains(&keyword.as_str())
            && !ALLOWED_ANNOTATIONS.contains(&keyword.as_str())
        {
            return Err(PluginError::ValidationFailed(format!(
                "unsupported plugin configuration schema keyword '{keyword}' at {path}"
            )));
        }
    }

    if let Some(expected) = schema.get("type") {
        let valid_type = match expected {
            Value::String(kind) => is_json_type_name(kind),
            Value::Array(kinds) => {
                !kinds.is_empty()
                    && kinds
                        .iter()
                        .all(|kind| kind.as_str().is_some_and(is_json_type_name))
                    && {
                        let mut unique = std::collections::HashSet::new();
                        kinds.iter().all(|kind| unique.insert(kind))
                    }
            }
            _ => false,
        };
        if !valid_type {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema has an invalid type at {path}"
            )));
        }
    }

    if let Some(enum_values) = schema.get("enum") {
        if !matches!(enum_values, Value::Array(values) if !values.is_empty()) {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema enum must be a non-empty array at {path}"
            )));
        }
    }

    if let Some(properties) = schema.get("properties") {
        let Some(properties) = properties.as_object() else {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema properties must be an object at {path}"
            )));
        };
        for (name, child_schema) in properties {
            validate_schema_definition(child_schema, format!("{path}.{name}"), depth + 1)?;
        }
    }

    if let Some(required) = schema.get("required") {
        let Some(required) = required.as_array() else {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema required must be an array at {path}"
            )));
        };
        let mut names = std::collections::HashSet::new();
        for name in required {
            let Some(name) = name.as_str() else {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration schema required names must be strings at {path}"
                )));
            };
            if !names.insert(name) {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration schema required names must be unique at {path}"
                )));
            }
        }
    }

    if let Some(additional) = schema.get("additionalProperties") {
        if !additional.is_boolean() {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema additionalProperties must be boolean at {path}"
            )));
        }
    }
    if let Some(items) = schema.get("items") {
        if items.is_array() {
            return Err(PluginError::ValidationFailed(format!(
                "tuple-style items are unsupported at {path}"
            )));
        }
        validate_schema_definition(items, format!("{path}[*]"), depth + 1)?;
    }

    let min_length = validate_nonnegative_integer_keyword(schema, "minLength", &path)?;
    let max_length = validate_nonnegative_integer_keyword(schema, "maxLength", &path)?;
    if let (Some(min), Some(max)) = (min_length, max_length) {
        if min > max {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema minLength exceeds maxLength at {path}"
            )));
        }
    }
    let minimum = validate_number_keyword(schema, "minimum", &path)?;
    let maximum = validate_number_keyword(schema, "maximum", &path)?;
    if let (Some(min), Some(max)) = (minimum, maximum) {
        if min > max {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration schema minimum exceeds maximum at {path}"
            )));
        }
    }

    Ok(())
}

fn validate_nonnegative_integer_keyword(
    schema: &serde_json::Map<String, Value>,
    keyword: &str,
    path: &str,
) -> Result<Option<u64>, PluginError> {
    let Some(value) = schema.get(keyword) else {
        return Ok(None);
    };
    let Some(value) = value.as_u64() else {
        return Err(PluginError::ValidationFailed(format!(
            "plugin configuration schema {keyword} must be a non-negative integer at {path}"
        )));
    };
    Ok(Some(value))
}

fn validate_number_keyword(
    schema: &serde_json::Map<String, Value>,
    keyword: &str,
    path: &str,
) -> Result<Option<f64>, PluginError> {
    let Some(value) = schema.get(keyword) else {
        return Ok(None);
    };
    let Some(value) = value.as_f64().filter(|number| number.is_finite()) else {
        return Err(PluginError::ValidationFailed(format!(
            "plugin configuration schema {keyword} must be a finite number at {path}"
        )));
    };
    Ok(Some(value))
}

fn is_json_type_name(kind: &str) -> bool {
    matches!(
        kind,
        "null" | "boolean" | "object" | "array" | "string" | "number" | "integer"
    )
}

fn validate_schema_node(
    schema: &Value,
    value: &Value,
    path: String,
    depth: usize,
) -> Result<(), PluginError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(PluginError::LimitExceeded(format!(
            "plugin configuration exceeds maximum nesting depth at {path}"
        )));
    }
    if let Some(boolean_schema) = schema.as_bool() {
        if boolean_schema {
            return Ok(());
        }
        return Err(PluginError::ValidationFailed(format!(
            "plugin configuration is rejected by schema at {path}"
        )));
    }

    let Some(schema) = schema.as_object() else {
        return Err(PluginError::ValidationFailed(format!(
            "plugin configuration schema is invalid at {path}"
        )));
    };

    if let Some(expected) = schema.get("type") {
        let matches_type = match expected {
            Value::String(kind) => json_type_matches(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| json_type_matches(kind, value)),
            _ => false,
        };
        if !matches_type {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration has the wrong type at {path}"
            )));
        }
    }

    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration is not an allowed value at {path}"
            )));
        }
    }
    if let Some(constant) = schema.get("const") {
        if constant != value {
            return Err(PluginError::ValidationFailed(format!(
                "plugin configuration does not match the required value at {path}"
            )));
        }
    }

    if let Some(string) = value.as_str() {
        if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
            if string.chars().count() < min as usize {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration string is too short at {path}"
                )));
            }
        }
        if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
            if string.chars().count() > max as usize {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration string is too long at {path}"
                )));
            }
        }
    }

    if let Some(number) = value.as_f64() {
        if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
            if number < minimum {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration number is below the minimum at {path}"
                )));
            }
        }
        if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
            if number > maximum {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration number is above the maximum at {path}"
                )));
            }
        }
    }

    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        let additional_allowed = schema
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(PluginError::ValidationFailed(format!(
                        "plugin configuration is missing required field '{name}' at {path}"
                    )));
                }
            }
        }
        for (name, child) in object {
            if let Some(property_schema) = properties.and_then(|items| items.get(name)) {
                validate_schema_node(property_schema, child, format!("{path}.{name}"), depth + 1)?;
            } else if !additional_allowed {
                return Err(PluginError::ValidationFailed(format!(
                    "plugin configuration contains unknown field '{name}' at {path}"
                )));
            }
        }
    }

    if let (Some(items_schema), Some(array)) = (schema.get("items"), value.as_array()) {
        for (index, child) in array.iter().enumerate() {
            validate_schema_node(items_schema, child, format!("{path}[{index}]"), depth + 1)?;
        }
    }

    Ok(())
}

fn json_type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        // serde_json represents 1.0 as a number even though JSON Schema
        // considers it an integer-valued number.
        "integer" => {
            value.as_i64().is_some()
                || value.as_u64().is_some()
                || value
                    .as_f64()
                    .is_some_and(|number| number.is_finite() && number.fract() == 0.0)
        }
        _ => false,
    }
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

fn validate_identifier(kind: &str, value: &str, max_bytes: usize) -> Result<(), PluginError> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(PluginError::ValidationFailed(format!(
            "{kind} must be non-empty and at most {max_bytes} bytes"
        )));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(PluginError::ValidationFailed(format!(
            "{kind} contains unsupported characters"
        )));
    }
    Ok(())
}

fn validate_text(kind: &str, value: &str, max_bytes: usize) -> Result<(), PluginError> {
    if value.len() > max_bytes {
        return Err(PluginError::ValidationFailed(format!(
            "{kind} exceeds {max_bytes} bytes"
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
