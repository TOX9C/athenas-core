//! MCP request dispatch and JSON-RPC recovery helpers.
//!
//! This module is intentionally transport-agnostic: TCP and stdio callers
//! share the same request semantics while `McpServer` owns lifecycle state.

use super::{
    get_tools, AgentCommsHandler, JsonRpcError, JsonRpcRequest, JsonRpcResponse, OutputHandler,
    SpawnHandler, TaskHandler,
};

pub(super) async fn handle_request_impl(
    token: &str,
    req: &JsonRpcRequest,
    task_handler: &Option<TaskHandler>,
    spawn_handler: &Option<SpawnHandler>,
    output_handler: &Option<OutputHandler>,
    agent_comms_handler: &Option<AgentCommsHandler>,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => {
            let params = &req.params;
            if params.get("token").and_then(|t| t.as_str()) != Some(token) {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: req.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid or missing auth token".into(),
                        data: None,
                    }),
                };
            }
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "athena-orchestrator", "version": "1.0.0" }
                })),
                error: None,
            }
        }
        "notifications/initialized" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: Some(serde_json::Value::Null),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: Some(serde_json::json!({ "tools": get_tools() })),
            error: None,
        },
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let result = handle_tool_call_impl(
                name,
                arguments,
                task_handler,
                spawn_handler,
                output_handler,
                agent_comms_handler,
            )
            .await;
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id.clone(),
                result: Some(result),
                error: None,
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
                data: None,
            }),
        },
    }
}

async fn handle_tool_call_impl(
    name: &str,
    args: serde_json::Value,
    task_handler: &Option<TaskHandler>,
    spawn_handler: &Option<SpawnHandler>,
    output_handler: &Option<OutputHandler>,
    agent_comms_handler: &Option<AgentCommsHandler>,
) -> serde_json::Value {
    match name {
        "notify" => {
            let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("info");
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Agent Notification");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            log::info!(
                "[MCP notify] level={}, title={}, msg={}",
                level,
                title,
                message
            );
            serde_json::json!({ "content": [{ "type": "text", "text": "Notification delivered." }] })
        }
        "status_update" => {
            let status = args
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("idle");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            log::info!("[MCP status_update] status={}, msg={}", status, message);
            serde_json::json!({ "content": [{ "type": "text", "text": format!("Status updated to: {}", status) }] })
        }
        "request_input" => {
            let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Input Request");
            log::info!("[MCP request_input] title={}, prompt={}", title, prompt);
            serde_json::json!({ "content": [{ "type": "text", "text": "Input request received. (Blocking input not yet available — use environment variables or config files for now.)" }] })
        }
        "create_tasks" => {
            if let Some(handler) = task_handler {
                handler("create_tasks", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'create_tasks' not yet implemented" }] })
            }
        }
        "get_next_task" => {
            if let Some(handler) = task_handler {
                handler("get_next_task", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'get_next_task' not yet implemented" }] })
            }
        }
        "update_task_status" => {
            if let Some(handler) = task_handler {
                handler("update_task_status", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'update_task_status' not yet implemented" }] })
            }
        }
        "spawn_agents" => {
            if let Some(handler) = spawn_handler {
                handler(&args)
            } else {
                let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                serde_json::json!({ "content": [{ "type": "text", "text": format!("Spawn request received for {} agents (placeholder — real implementation requires PTY access)", count) }] })
            }
        }
        "get_output" => {
            if let Some(handler) = output_handler {
                handler("get_output", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'get_output' not yet implemented" }] })
            }
        }
        "list_agent_panes" => {
            if let Some(handler) = output_handler {
                handler("list_agent_panes", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'list_agent_panes' not yet implemented" }] })
            }
        }
        "athena_forward_output" => {
            let entries = args
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let session_id = args.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
            log::info!(
                "[MCP athena_forward_output] entries={}, session={}",
                entries,
                session_id
            );
            serde_json::json!({ "content": [{ "type": "text", "text": format!("Forwarded {} output entries.", entries) }] })
        }
        "send_message_to_agent" => {
            if let Some(handler) = agent_comms_handler {
                handler("send_message_to_agent", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'send_message_to_agent' not yet implemented" }] })
            }
        }
        "read_agent_messages" => {
            if let Some(handler) = agent_comms_handler {
                handler("read_agent_messages", &args)
            } else {
                serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Tool 'read_agent_messages' not yet implemented" }] })
            }
        }
        "code_search" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            let glob = args
                .get("glob")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let context_lines = args
                .get("context_lines")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let options = crate::types::SearchOptions {
                pattern,
                path,
                glob,
                case_sensitive,
                max_results,
                context_lines,
            };

            let search_result = crate::search::search_code(&options).await;

            match search_result {
                Ok(result) => {
                    if result.matches.is_empty() {
                        serde_json::json!({ "content": [{ "type": "text", "text": format!("No matches found for pattern \"{}\" in {}.", options.pattern, options.path) }] })
                    } else {
                        let formatted = result
                            .matches
                            .iter()
                            .map(|m| {
                                let mut output = format!(
                                    "{}:{}:{}: {}",
                                    m.file_path, m.line_number, m.column, m.line_text
                                );
                                if !m.context_before.is_empty() {
                                    let before = m
                                        .context_before
                                        .iter()
                                        .enumerate()
                                        .map(|(i, l)| {
                                            format!(
                                                "  {}: {}",
                                                m.line_number - m.context_before.len() as u32
                                                    + i as u32,
                                                l
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    output = format!("{}\n{}", before, output);
                                }
                                if !m.context_after.is_empty() {
                                    let after = m
                                        .context_after
                                        .iter()
                                        .enumerate()
                                        .map(|(i, l)| {
                                            format!("  {}: {}", m.line_number + 1 + i as u32, l)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    output = format!("{}\n{}", output, after);
                                }
                                output
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n");

                        let header = format!(
                            "Found {} matches in {} files{}:\n\n",
                            result.stats.total_matches,
                            result.stats.files_matched,
                            if result.truncated { " (truncated)" } else { "" }
                        );

                        serde_json::json!({ "content": [{ "type": "text", "text": format!("{}{}", header, formatted) }] })
                    }
                }
                Err(e) => {
                    serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": format!("Search error: {}", e) }] })
                }
            }
        }
        "search_files" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            let glob = args
                .get("glob")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let search_result =
                crate::search::search_files(&path, &pattern, glob.as_deref(), max_results).await;

            match search_result {
                Ok(results) => {
                    if results.is_empty() {
                        serde_json::json!({ "content": [{ "type": "text", "text": format!("No files found matching pattern \"{}\" in {}.", pattern, path) }] })
                    } else {
                        let formatted = results.join("\n");
                        serde_json::json!({ "content": [{ "type": "text", "text": format!("Found {} files:\n\n{}", results.len(), formatted) }] })
                    }
                }
                Err(e) => {
                    serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": format!("Search error: {}", e) }] })
                }
            }
        }
        _ => {
            serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": format!("Unknown tool: {}", name) }] })
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions for tool executor delegation
// ---------------------------------------------------------------------------

/// Build a JSON-RPC 2.0 Parse error response for a malformed request.
///
/// The `id` is best-effort recovered from the raw payload — if the buffer
/// is not even valid JSON, the id is `null` per the spec. The error code
/// `-32700` is the standardized JSON-RPC Parse error.
pub(super) fn make_parse_error_response(raw: &str) -> String {
    // Try full parse first. If that fails (truncated/garbled input), fall
    // back to a tolerant scan for the first `"id": <value>` pair in the
    // buffer. Spec-compliant: id falls back to null if unrecoverable.
    let id = serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .or_else(|| extract_id_from_partial(raw))
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32700,
            "message": "Parse error"
        }
    })
    .to_string()
}

/// Tolerant `id` extraction from a (possibly truncated) JSON buffer.
///
/// Looks for the first `"id"` key in the buffer and parses the scalar
/// that follows. Returns `None` if no plausible id can be recovered.
fn extract_id_from_partial(raw: &str) -> Option<serde_json::Value> {
    // Search for the literal `"id"` pattern. For each occurrence check
    // the key is in a valid object position, then read a single scalar
    // value (number, bool, null, or string — possibly truncated).
    let bytes = raw.as_bytes();
    let mut search_start = 0;
    while let Some(rel) = raw[search_start..].find("\"id\"") {
        let key_pos = search_start + rel;
        // The character just before `"id"` should be a key-separator in
        // valid object syntax: `{`, `,`, or whitespace.
        let before_ok = key_pos == 0
            || matches!(
                bytes[key_pos - 1],
                b'{' | b',' | b' ' | b'\n' | b'\r' | b'\t'
            );
        if !before_ok {
            search_start = key_pos + 4;
            continue;
        }
        // j is just past the closing quote of "id"; expect `:` or whitespace.
        let mut j = key_pos + 4;
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b':' {
            search_start = key_pos + 4;
            continue;
        }
        j += 1;
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        if j >= bytes.len() {
            return None;
        }
        // Read a single scalar value starting at j.
        return parse_scalar_at(raw, j);
    }
    None
}

/// Read one JSON scalar (number, bool, null, or string — possibly
/// truncated) starting at byte offset `j` of `raw`. Returns `None` if
/// no recognizable scalar can be recovered.
fn parse_scalar_at(raw: &str, j: usize) -> Option<serde_json::Value> {
    let bytes = raw.as_bytes();
    if j >= bytes.len() {
        return None;
    }
    let b = bytes[j];
    // Quoted string
    if b == b'"' {
        let mut k = j + 1;
        loop {
            if k >= bytes.len() {
                // Truncated string — return what we have.
                let s = &raw[j + 1..];
                return Some(serde_json::Value::String(s.to_string()));
            }
            match bytes[k] {
                b'\\' if k + 1 < bytes.len() => k += 2,
                b'"' => {
                    let s = &raw[j + 1..k];
                    // Wrap the captured body in quotes and re-parse to
                    // honour JSON escape sequences.
                    let quoted = format!("\"{}\"", s.replace('"', "\\\""));
                    return serde_json::from_str(&quoted).ok();
                }
                _ => k += 1,
            }
        }
    }
    // Null / true / false
    if b == b'n' || b == b't' || b == b'f' {
        let tail = &raw[j..];
        for kw in &["null", "true", "false"] {
            if tail.starts_with(kw) {
                return serde_json::from_str(kw).ok();
            }
        }
        return None;
    }
    // Number: scan digits, sign, dot, exponent.
    if b == b'-' || b.is_ascii_digit() {
        let mut k = j;
        if bytes[k] == b'-' {
            k += 1;
        }
        while k < bytes.len()
            && (bytes[k].is_ascii_digit()
                || bytes[k] == b'.'
                || bytes[k] == b'e'
                || bytes[k] == b'E'
                || bytes[k] == b'+'
                || bytes[k] == b'-')
        {
            k += 1;
        }
        let n = &raw[j..k];
        return serde_json::from_str(n).ok();
    }
    None
}

/// Map MCP tool names to ToolExecutor tool names.
pub(super) fn map_mcp_to_executor_name(mcp_name: &str) -> &str {
    match mcp_name {
        "create_tasks" => "kanban_create_task",
        "get_next_task" => "kanban_list_tasks",
        "update_task_status" => "kanban_update_task",
        "spawn_agents" => "launch_builtin_agent",
        "get_output" => "read_agent_output",
        "list_agent_panes" => "list_agents",
        "code_search" => "fs_search",
        "search_files" => "fs_search",
        "run_command_in_terminals" => "run_command_in_terminals",
        "close_terminals" => "close_terminals",
        "prompt_agent" => "prompt_agent",
        "launch_builtin_agent" => "launch_builtin_agent",
        _ => mcp_name,
    }
}

/// Convert JSON-RPC tool call arguments into a `ToolInput` structure,
/// handling both camelCase and snake_case keys.
#[allow(clippy::field_reassign_with_default)]
pub(super) fn args_to_tool_input(
    args: &serde_json::Value,
) -> Option<crate::tool_executor::ToolInput> {
    let map = args.as_object()?;

    let mut ti = crate::tool_executor::ToolInput::default();

    // Kanban
    ti.title = map
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    ti.description = map
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    ti.status = map
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if let Some(v) = map.get("taskId").or_else(|| map.get("task_id")) {
        ti.task_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("spaceId").or_else(|| map.get("space_id")) {
        ti.space_id = v.as_str().map(|s| s.to_string());
    }

    // Agent / Pane
    if let Some(v) = map.get("agentType").or_else(|| map.get("agent_type")) {
        ti.agent_type = v.as_str().map(|s| s.to_string());
    }
    if let Some(n) = map
        .get("agentCount")
        .or_else(|| map.get("agent_count"))
        .and_then(|v| v.as_u64())
    {
        ti.agent_count = Some(n as u32);
    }
    if let Some(v) = map.get("taskPrompt").or_else(|| map.get("task_prompt")) {
        ti.task_prompt = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("command").and_then(|v| v.as_str()) {
        ti.command = Some(v.to_string());
    }
    if let Some(v) = map.get("paneId").or_else(|| map.get("pane_id")) {
        ti.pane_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("agentId").or_else(|| map.get("agent_id")) {
        ti.agent_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(arr) = map.get("paneIds").or_else(|| map.get("pane_ids")) {
        if let Some(arr) = arr.as_array() {
            ti.pane_ids = Some(
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
            );
        }
    }

    // FS / Search
    if let Some(v) = map.get("path").and_then(|v| v.as_str()) {
        ti.path = Some(v.to_string());
    }
    if let Some(v) = map.get("pattern").and_then(|v| v.as_str()) {
        ti.pattern = Some(v.to_string());
    }
    if let Some(n) = map.get("limit").and_then(|v| v.as_u64()) {
        ti.limit = Some(n as usize);
    }
    if let Some(n) = map
        .get("sinceLine")
        .or_else(|| map.get("since_line"))
        .and_then(|v| v.as_u64())
    {
        ti.since_line = Some(n as u32);
    }

    // Plan
    if let Some(v) = map.get("goal").and_then(|v| v.as_str()) {
        ti.goal = Some(v.to_string());
    }
    if let Some(v) = map.get("reasoning").and_then(|v| v.as_str()) {
        ti.reasoning = Some(v.to_string());
    }
    if let Some(v) = map.get("stepId").or_else(|| map.get("step_id")) {
        ti.step_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("planId").or_else(|| map.get("plan_id")) {
        ti.plan_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("prompt").and_then(|v| v.as_str()) {
        ti.prompt = Some(v.to_string());
    }
    if let Some(v) = map
        .get("overallStatus")
        .or_else(|| map.get("overall_status"))
    {
        ti.overall_status = v.as_str().map(|s| s.to_string());
    }
    if let Some(arr) = map
        .get("stepEvaluations")
        .or_else(|| map.get("step_evaluations"))
    {
        if let Some(arr) = arr.as_array() {
            ti.step_evaluations = Some(arr.clone());
        }
    }
    if let Some(v) = map.get("nextAction").or_else(|| map.get("next_action")) {
        ti.next_action = v.as_str().map(|s| s.to_string());
    }

    // Misc
    if let Some(v) = map.get("question").and_then(|v| v.as_str()) {
        ti.question = Some(v.to_string());
    }
    if let Some(arr) = map.get("options") {
        if let Some(arr) = arr.as_array() {
            ti.options = Some(arr.clone());
        }
    }
    if let Some(v) = map.get("message").and_then(|v| v.as_str()) {
        ti.message = Some(v.to_string());
    }
    if let Some(v) = map
        .get("targetAgentId")
        .or_else(|| map.get("target_agent_id"))
    {
        ti.target_agent_id = v.as_str().map(|s| s.to_string());
    }
    if let Some(v) = map.get("messageType").or_else(|| map.get("message_type")) {
        ti.message_type = v.as_str().map(|s| s.to_string());
    }

    Some(ti)
}

#[cfg(test)]
mod tests_parse_error {
    use super::*;

    #[test]
    fn parse_error_response_includes_id() {
        // Truncated JSON with a valid `id` field at the start
        let raw = r#"{"id":42,"method":"foo"#;
        let resp = make_parse_error_response(raw);
        assert!(resp.contains("\"id\":42"), "expected id 42 in: {}", resp);
        assert!(
            resp.contains("-32700"),
            "expected parse error code in: {}",
            resp
        );
        assert!(
            resp.contains("\"jsonrpc\":\"2.0\""),
            "expected jsonrpc 2.0 in: {}",
            resp
        );
    }

    #[test]
    fn parse_error_response_with_no_id() {
        // Completely unparseable input
        let raw = "not json at all";
        let resp = make_parse_error_response(raw);
        assert!(
            resp.contains("\"id\":null"),
            "expected null id in: {}",
            resp
        );
        assert!(
            resp.contains("-32700"),
            "expected parse error code in: {}",
            resp
        );
    }

    #[test]
    fn parse_error_response_is_valid_json() {
        let raw = r#"{"id":"abc","method":"x"#;
        let resp = make_parse_error_response(raw);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("response must be valid JSON");
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], "abc");
        assert_eq!(parsed["error"]["code"], -32700);
        assert_eq!(parsed["error"]["message"], "Parse error");
    }

    #[test]
    fn parse_error_response_falls_back_to_null_when_id_missing() {
        // Valid JSON object but no `id` key
        let raw = r#"{"method":"foo"}"#;
        let resp = make_parse_error_response(raw);
        assert!(
            resp.contains("\"id\":null"),
            "expected null id in: {}",
            resp
        );
    }
}
